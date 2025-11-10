use fedimint_bip39::{Bip39RootSecretStrategy, Mnemonic};
use fedimint_client::meta::MetaService;
use fedimint_client::module::meta::LegacyMetaSource;
use fedimint_client::secret::RootSecretStrategy;
use fedimint_client::{Client, ClientHandle, ClientModuleInstance, RootSecret};
use fedimint_core::anyhow::{Context, anyhow, bail, ensure};
use fedimint_core::bitcoin::hashes::sha256;
use fedimint_core::core::OperationId;
use fedimint_core::db::{Database, IRawDatabaseExt};
use fedimint_core::invite_code::InviteCode;
use fedimint_core::{Amount, anyhow};
use fedimint_ln_client::{
    LightningClientInit, LightningClientModule, LightningOperationMeta,
    LightningOperationMetaVariant, LnReceiveState,
};
use fedimint_meta_client::MetaModuleMetaSourceWithFallback;
use fedimint_mint_client::MintClientInit;
use futures_lite::stream::StreamExt;
use lightning_invoice::{Bolt11Invoice, Bolt11InvoiceDescription, Description};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

const ECASH_CLUB_INVITE: &str = "fed11qgqzggnhwden5te0v9cxjtn9vd3jue3wvfkxjmnyva6kzunyd9skutnwv46z7qqpyzhv5mxgpl79xz7j649sj6qldmde5s2uxchy4uh7840qgymsqmazzp6sn43";

pub struct FedimintBuilder {
    datadir: PathBuf,
    federation: InviteCode,
}

impl Default for FedimintBuilder {
    fn default() -> Self {
        let xdg = xdg::BaseDirectories::new();

        Self {
            datadir: xdg
                .data_home
                .expect("Could not determine XDG data home")
                .join("fedimint/default"),
            federation: InviteCode::from_str(ECASH_CLUB_INVITE).expect("can be parsed"),
        }
    }
}

impl FedimintBuilder {
    /// Sets the directory where Fedimint data will be stored. Defaults to `$XDG_DATA_HOME/fedimint/default`
    pub fn datadir(mut self, path: PathBuf) -> Self {
        self.datadir = path;
        self
    }

    /// Sets the federation to connect to via an already parsed invite code. If you have a string invite code, use [`Self::federation`] instead.
    pub fn federation_invite(mut self, invite: InviteCode) -> Self {
        self.federation = invite;
        self
    }

    /// Sets the federation to connect to via an invite code string. If you already have a parsed invite code, use [`Self::federation_invite`] instead.
    pub fn federation(mut self, invite: &str) -> anyhow::Result<Self> {
        let invite = InviteCode::from_str(invite)?;
        self.federation = invite;
        Ok(self)
    }

    pub async fn build(self) -> anyhow::Result<Fedimint> {
        let mut client_builder = fedimint_client::Client::builder().await?;
        client_builder.with_module(MintClientInit);
        client_builder.with_module(LightningClientInit::default());
        let mut client_builder = client_builder.with_iroh_enable_next(false);
        client_builder.with_meta_service(MetaService::new(MetaModuleMetaSourceWithFallback::<
            LegacyMetaSource,
        >::default()));

        let db = fedimint_rocksdb::RocksDb::open(self.datadir)
            .await?
            .into_database();

        // TODO: use config being present to decide if to open or join
        let client = if let Some(root_secret) = try_load_root_secret(&db).await? {
            client_builder.open(db, root_secret).await?
        } else {
            let root_secret = generate_root_secret(&db).await?;
            client_builder
                .preview(&self.federation)
                .await?
                .join(db, root_secret)
                .await?
        };

        Ok(Fedimint { client })
    }
}

async fn try_load_root_secret(db: &Database) -> anyhow::Result<Option<RootSecret>> {
    let Some(entropy) = Client::load_decodable_client_secret_opt::<Vec<u8>>(&db).await? else {
        return Ok(None);
    };

    let mnemonic = Mnemonic::from_entropy(&entropy)?;

    Ok(Some(RootSecret::StandardDoubleDerive(
        Bip39RootSecretStrategy::<12>::to_root_secret(&mnemonic),
    )))
}

async fn generate_root_secret(db: &Database) -> anyhow::Result<RootSecret> {
    let mnemonic = Mnemonic::generate(12)?;
    let entropy = mnemonic.to_entropy();

    Client::store_encodable_client_secret(&db, &entropy).await?;

    Ok(RootSecret::StandardDoubleDerive(Bip39RootSecretStrategy::<
        12,
    >::to_root_secret(
        &mnemonic
    )))
}

pub struct Fedimint {
    client: ClientHandle,
}

impl Fedimint {

    pub fn builder() -> FedimintBuilder {
        FedimintBuilder::default()
    }

    pub async fn new() -> anyhow::Result<Self> {
        Self::builder().build().await
    }

    pub fn client(&self) -> &ClientHandle {
        &self.client
    }

    fn ln_module(&self) -> ClientModuleInstance<'_, LightningClientModule> {
        self.client
            .get_first_module::<LightningClientModule>()
            .expect("LN module not found")
    }

    pub async fn lightning_invoice(
        &self,
        amount_msats: u64,
        description: &str,
    ) -> anyhow::Result<Bolt11Invoice> {
        let ln_client = self.ln_module();

        let ln_gateway = ln_client
            .get_gateway(None, false)
            .await?
            .ok_or_else(|| anyhow!("No LN gateway available"))?;
        let (_, invoice, _) = ln_client
            .create_bolt11_invoice(
                Amount::from_msats(amount_msats),
                Bolt11InvoiceDescription::Direct(Description::new(description.into())?),
                None,
                (),
                Some(ln_gateway),
            )
            .await?;

        Ok(invoice)
    }

    pub async fn await_payment(&self, invoice: &Bolt11Invoice) -> anyhow::Result<()> {
        self.await_payment_by_hash(invoice.payment_hash()).await
    }

    pub async fn await_payment_by_hash(&self, payment_hash: &sha256::Hash) -> anyhow::Result<()> {
        let operation_id = OperationId(*payment_hash.as_ref());

        let operation = self
            .client
            .operation_log()
            .get_operation(operation_id)
            .await
            .context(
                "No operation found for payment hash, was the invoice issued by us?".to_string(),
            )?;
        ensure!(
            operation.operation_module_kind() == "ln",
            "Operation associated with payment hash is not an LN operation"
        );

        let operation_meta = operation.meta::<LightningOperationMeta>();
        ensure!(
            matches!(
                operation_meta.variant,
                LightningOperationMetaVariant::Receive { .. }
            ),
            "Operation associated with the payment hash is not an incoming payment"
        );

        let ln_module = self.ln_module();
        let mut update_stream = ln_module
            .subscribe_ln_receive(operation_id)
            .await
            .context("Unexpected error subscribing to operation")?
            .into_stream();
        while let Some(update) = update_stream.next().await {
            match update {
                LnReceiveState::Canceled { reason } => {
                    return Err(anyhow!("Payment was canceled: {}", reason));
                }
                LnReceiveState::Claimed => {
                    return Ok(());
                }
                _ => {}
            }
        }

        unreachable!("Stream ended unexpectedly");
    }
}
