#![no_std]
use shared::error::SharedContractError;
use shared::validation::require_within_bounds;
use shared::{extend_persistent_ttl, require_admin, TTL_THRESHOLD, TTL_TARGET};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, String, Symbol, Vec,
};

pub use shared::error::SharedContractError as SharedError;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    CredentialAlreadyRevoked = 1,
    UnauthorizedAdmin = 2,
    EngineerNotFound = 3,
    NotInitialized = 4,
    AdminAlreadyInitialized = 5,
    UntrustedIssuer = 6,
    InvalidCredentialHash = 7,
    Paused = 8,
    CredentialRevoked = 9,
    EngineerAlreadyRegistered = 10,
    IssuerNotFound = 11,
    PendingAdminAlreadyExists = 12,
    InvalidValidityPeriod = 13,
    IssuerRemoved = 14,
    TimelockNotExpired = 15,
    ProposalNotFound = 16,
    CredentialSuspended = 17,
    EngineerAlreadySuspended = 18,
    InvalidSuspensionPeriod = 19,
    BatchRevokeTooLarge = 17,
    CredentialExpired = 18,
}

impl From<SharedContractError> for ContractError {
    fn from(e: SharedContractError) -> Self {
        match e {
            SharedContractError::NotInitialized => ContractError::NotInitialized,
            SharedContractError::AlreadyInitialized => ContractError::AdminAlreadyInitialized,
            SharedContractError::UnauthorizedAdmin => ContractError::UnauthorizedAdmin,
            SharedContractError::Paused => ContractError::Paused,
            SharedContractError::TimelockNotExpired => ContractError::TimelockNotExpired,
            SharedContractError::ProposalNotFound => ContractError::ProposalNotFound,
            SharedContractError::PendingAdminAlreadyExists => ContractError::PendingAdminAlreadyExists,
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Engineer {
    pub address: Address,
    pub credential_hash: BytesN<32>,
    pub issuer: Address,
    pub active: bool,
    pub issued_at: u64,
    pub expires_at: u64,
    /// Unix timestamp until which the engineer is suspended; `None` means not suspended.
    pub suspension_end_time: Option<u64>,
    pub reputation_score: u32,
    pub notes: Option<soroban_sdk::String>,
    pub specializations: Vec<Symbol>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineerStatus {
    Active = 0,
    Revoked = 1,
    Expired = 2,
    NotFound = 3,
    Suspended = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CredentialStatus {
    Valid = 0,
    GracePeriod = 1,
    HardExpired = 2,
    Revoked = 3,
    NotFound = 4,
    Suspended = 5,
    Expired = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelockProposal {
    pub proposed_at: u64,
    pub executed: bool,
}

fn engineer_key(addr: &Address) -> (Symbol, Address) {
    (symbol_short!("ENG"), addr.clone())
}

fn revoke_timelock_key(engineer: &Address) -> (Symbol, Address) {
    (symbol_short!("TL_RVK"), engineer.clone())
}

const PAUSED_KEY: Symbol = symbol_short!("PAUSED");
const ENGINEER_COUNT: Symbol = symbol_short!("ENG_CNT");
#[allow(dead_code)]
const REG_ENG_TOPIC: Symbol = symbol_short!("REG_ENG");
const REVOKE_TOPIC: Symbol = symbol_short!("REV_CRED");
const SUSPEND_TOPIC: Symbol = symbol_short!("SUSP_ENG");
#[allow(dead_code)]
const UNSUSPEND_TOPIC: Symbol = symbol_short!("UNSUSP_E");
const MIN_VALIDITY_PERIOD: u64 = 86_400;
const EVENT_PROP_ADMIN: Symbol = symbol_short!("PROP_ADM");
const TIMELOCK_DELAY_SECS: u64 = 48 * 60 * 60;
/// Grace period allowing engineers to work after credential expiry (7 days).
const GRACE_PERIOD_SECS: u64 = 7 * 86_400;
const GRACE_PERIOD_KEY: Symbol = symbol_short!("GRACE_P");
const MAX_BATCH_REVOKE: u32 = 50;

fn is_paused(env: &Env) -> bool {
    env.storage().persistent().get(&PAUSED_KEY).unwrap_or(false)
}

fn ensure_not_paused(env: &Env) {
    if is_paused(env) {
        panic_with_error!(env, ContractError::Paused);
    }
}

/// Returns `true` if the engineer is currently within a suspension window.
fn is_suspended(record: &Engineer, now: u64) -> bool {
    match record.suspension_end_time {
        Some(end) => now < end,
        None => false,
    }
}

fn require_revoke_timelock_ready(env: &Env, engineer: &Address) {
    let key = revoke_timelock_key(engineer);
    let mut proposal: TimelockProposal = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::ProposalNotFound));
    if proposal.executed {
        panic_with_error!(env, ContractError::ProposalNotFound);
    }
    if env
        .ledger()
        .timestamp()
        .saturating_sub(proposal.proposed_at)
        < TIMELOCK_DELAY_SECS
    {
        panic_with_error!(env, ContractError::TimelockNotExpired);
    }
    proposal.executed = true;
    env.storage().persistent().set(&key, &proposal);
    extend_persistent_ttl(&env, &key);
}

fn upgrade_timelock_key() -> (Symbol, Symbol) {
    (symbol_short!("TL_GLOB"), symbol_short!("UPGRADE"))
}

fn require_upgrade_timelock_ready(env: &Env) {
    let key = upgrade_timelock_key();
    let mut proposal: TimelockProposal = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| panic_with_error!(env, ContractError::ProposalNotFound));
    if proposal.executed {
        panic_with_error!(env, ContractError::ProposalNotFound);
    }
    if env
        .ledger()
        .timestamp()
        .saturating_sub(proposal.proposed_at)
        < TIMELOCK_DELAY_SECS
    {
        panic_with_error!(env, ContractError::TimelockNotExpired);
    }
    proposal.executed = true;
    env.storage().persistent().set(&key, &proposal);
    extend_persistent_ttl(&env, &key);
}

fn admin_key() -> Symbol {
    symbol_short!("ADMIN")
}

fn pending_admin_key() -> Symbol {
    symbol_short!("PADMIN")
}

fn trusted_key(issuer: &Address) -> (Symbol, Address) {
    (symbol_short!("TRUSTED"), issuer.clone())
}

fn issuer_engineers_key(issuer: &Address) -> (Symbol, Address) {
    (symbol_short!("ISS_ENGS"), issuer.clone())
}

/// Returns the key for the authoritative trusted-issuer list in instance storage.
/// This list MUST NOT expire: TTL must be extended on every write so that
/// `get_trusted_issuers` never returns a stale empty vec while individual
/// `trusted_key` entries are still active.
fn issuer_list_key() -> Symbol {
    symbol_short!("ISS_LIST")
}

#[contract]
pub struct EngineerRegistry;

#[contractimpl]
impl EngineerRegistry {
    /// Propose the revocation of an engineer's credential.
    /// The revocation is subject to a timelock before it can be executed.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer whose credential is being revoked
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if the engineer record does not exist
    pub fn propose_revoke_credential(env: Env, engineer: Address) {
        ensure_not_paused(&env);
        let record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));
        record.issuer.require_auth();
        let key = revoke_timelock_key(&engineer);
        env.storage().persistent().set(
            &key,
            &TimelockProposal {
                proposed_at: env.ledger().timestamp(),
                executed: false,
            },
        );
        extend_persistent_ttl(&env, &key);
    }

    /// Execute a pending engineer credential revocation after its timelock has expired.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer whose credential revocation is being executed
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if the engineer record does not exist
    /// - [`ContractError::TimelockNotReady`] if the revocation timelock is not yet ready
    pub fn execute_revoke_credential(env: Env, engineer: Address) {
        require_revoke_timelock_ready(&env, &engineer);
        Self::revoke_credential(env, engineer);
    }

    /// Register a new engineer with their credential information.
    /// Only trusted issuers can register engineers.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer being registered
    /// * `credential_hash` - SHA-256 hash of the engineer's credentials (32 bytes; as hex string: 64 characters)
    /// * `issuer` - The trusted issuer address registering the engineer
    /// * `validity_period` - Duration in seconds for which the credentials are valid
    /// * `notes` - Optional specialization note (e.g. "Certified: High-Voltage Generators")
    ///
    /// # Panics
    /// - [`ContractError::UntrustedIssuer`] if the issuer is not in the trusted list
    /// - [`ContractError::InvalidCredentialHash`] if credential hash is all zeros
    /// - [`ContractError::EngineerAlreadyRegistered`] if an active engineer record already exists
    pub fn register_engineer(
        env: Env,
        engineer: Address,
        credential_hash: BytesN<32>,
        issuer: Address,
        validity_period: u64,
        notes: Option<String>,
    ) {
        ensure_not_paused(&env);
        issuer.require_auth();
        if !env.storage().instance().has(&trusted_key(&issuer)) {
            panic_with_error!(&env, ContractError::UntrustedIssuer);
        }
        if credential_hash == BytesN::from_array(&env, &[0u8; 32]) {
            panic_with_error!(&env, ContractError::InvalidCredentialHash);
        }
        if validity_period == 0 {
            panic_with_error!(&env, ContractError::InvalidValidityPeriod);
        }
        require_within_bounds(
            validity_period,
            MIN_VALIDITY_PERIOD,
            u64::MAX,
            "validity_period",
        );

        // Check if an engineer record already exists and is *not revoked*.
        // Re-registering would otherwise silently overwrite credentials.
        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
        {
            if existing.active {
                panic_with_error!(&env, ContractError::EngineerAlreadyRegistered);
            }
            // existing is present but not active (revoked) => allow re-registration.
        }

        let now = env.ledger().timestamp();
        let record = Engineer {
            address: engineer.clone(),
            credential_hash: credential_hash.clone(),
            issuer: issuer.clone(),
            active: true,
            issued_at: now,
            expires_at: now + validity_period,
            suspension_end_time: None,
            reputation_score: 0,
            notes,
            specializations: Vec::new(&env),
        };
        env.storage()
            .persistent()
            .set(&engineer_key(&engineer), &record);
        extend_persistent_ttl(&env, &engineer_key(&engineer));

        // Track issuer → engineers mapping (avoid duplicates on re-registration after revoke)
        let mut list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&issuer_engineers_key(&issuer))
            .unwrap_or(Vec::new(&env));
        if !list.contains(engineer.clone()) {
            list.push_back(engineer.clone());
        }
        env.storage()
            .persistent()
            .set(&issuer_engineers_key(&issuer), &list);
        extend_persistent_ttl(&env, &issuer_engineers_key(&issuer));

        // Increment engineer count
        let count: u32 = env.storage().persistent().get(&ENGINEER_COUNT).unwrap_or(0);
        env.storage().persistent().set(&ENGINEER_COUNT, &(count + 1));
        extend_persistent_ttl(&env, &ENGINEER_COUNT);
        env.storage()
            .persistent()
            .set(&ENGINEER_COUNT, &(count + 1));
        env.storage()
            .persistent()
            .extend_ttl(&ENGINEER_COUNT, TTL_THRESHOLD, TTL_TARGET);

        // Emit engineer registration event
        env.events().publish(
            (symbol_short!("reg_eng"),),
            (
                engineer.clone(),
                credential_hash.clone(),
                issuer.clone(),
                now,
            ),
        );
    }

    /// Verify if an engineer has valid, active credentials with detailed status.
    /// Distinguishes between valid, expired, revoked, and never-registered engineers.
    ///
    /// This is a read-only call and intentionally bypasses the pause guard.
    /// Blocking reads during a pause would prevent the lifecycle contract from
    /// checking credentials at all, which is worse than returning a stale result.
    /// Write operations (register, revoke, renew) remain blocked while paused.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer to verify
    ///
    /// # Returns
    /// A CredentialStatus enum:
    /// - `CredentialStatus::Valid` if the engineer has active, non-expired credentials
    /// - `CredentialStatus::HardExpired` if the engineer exists but credentials are expired
    /// - `CredentialStatus::Revoked` if the engineer exists but credentials are revoked
    /// - `CredentialStatus::NotFound` if the engineer was never registered
    pub fn verify_engineer(env: Env, engineer: Address) -> CredentialStatus {
        match env
            .storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
        {
            Some(e) => {
                if !e.active {
                    CredentialStatus::Revoked
                } else if is_suspended(&e, env.ledger().timestamp()) {
                    CredentialStatus::Suspended
                } else if !env.storage().instance().has(&trusted_key(&e.issuer)) {
                    // The issuer that credentialed this engineer is no longer trusted.
                    CredentialStatus::Revoked
                } else if env.ledger().timestamp() < e.expires_at {
                    CredentialStatus::Valid
                } else {
                    CredentialStatus::HardExpired
                }
            }
            None => CredentialStatus::NotFound,
        }
    }

    /// Verify multiple engineers in a single call.
    /// Results are returned in the same order as the input vec.
    ///
    /// # Arguments
    /// * `engineers` - Vec of engineer addresses to verify
    ///
    /// # Returns
    /// `Vec<CredentialStatus>` where each element indicates the credential status
    /// of the corresponding engineer (Valid, Expired, Revoked, or NotFound)
    pub fn batch_verify_engineers(env: Env, engineers: Vec<Address>) -> Vec<CredentialStatus> {
        let now = env.ledger().timestamp();
        let mut results: Vec<CredentialStatus> = Vec::new(&env);
        for engineer in engineers.iter() {
            let status = match env
                .storage()
                .persistent()
                .get::<_, Engineer>(&engineer_key(&engineer))
            {
                Some(e) => {
                    if !e.active {
                        CredentialStatus::Revoked
                    } else if is_suspended(&e, now) {
                        CredentialStatus::Suspended
                    } else if now < e.expires_at {
                        CredentialStatus::Valid
                    } else {
                        CredentialStatus::HardExpired
                    }
                }
                None => CredentialStatus::NotFound,
            };
            results.push_back(status);
        }
        results
    }

    /// Revoke an engineer's credentials, making them inactive.
    /// Only the original issuer can revoke credentials.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer whose credentials should be revoked
    ///
    /// # Authorization
    /// Requires signature from the original issuer stored in the engineer's record.
    /// A different trusted issuer cannot revoke another issuer's engineer.
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if no engineer exists with the given address
    /// - [`ContractError::CredentialAlreadyRevoked`] if the credentials are already revoked
    pub fn revoke_credential(env: Env, engineer: Address) {
        ensure_not_paused(&env);
        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));
        record.issuer.require_auth();
        if !record.active {
            panic_with_error!(&env, ContractError::CredentialAlreadyRevoked);
        }
        let _credential_hash = record.credential_hash.clone();
        let _revoked_by = record.issuer.clone();
        // Extend TTL before write to ensure consistency even on near-expired entries
        extend_persistent_ttl(&env, &engineer_key(&engineer));
        record.active = false;
        env.storage()
            .persistent()
            .set(&engineer_key(&engineer), &record);

        // Emit credential revocation event
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("REV_CRED")),
            (
                record.issuer.clone(),
                timestamp,
                engineer.clone(),
            ),
        );
        env.events().publish(
            (REVOKE_TOPIC, engineer.clone()),
            (
                engineer.clone(),
                record.credential_hash.clone(),
                record.issuer.clone(),
                timestamp,
            ),
        );
    }

    /// Renew an engineer's credential by extending the expiry.
    /// Only the original issuer can renew credentials.
    ///
    /// ## Renewal semantics
    ///
    /// The new `expires_at` is calculated as:
    /// - **Not yet expired or in grace period**: `current expires_at + new_validity_period`
    ///   (remaining validity is preserved; the new period is stacked on top)
    /// - **Hard-expired**: Renewal is rejected; re-issuance is required
    /// - **Revoked**: Renewal is rejected
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer whose credential should be renewed
    /// * `new_validity_period` - Duration in seconds to add to the credential's expiry
    ///   (stacked on top of remaining validity when not hard-expired)
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if no engineer exists with the given address
    /// - [`ContractError::CredentialRevoked`] if the credential has been revoked
    /// - [`ContractError::CredentialExpired`] if the credential is hard-expired (re-issuance required)
    /// - [`ContractError::IssuerRemoved`] if the issuer is no longer trusted
    /// - [`ContractError::InvalidValidityPeriod`] if `new_validity_period` is below the minimum
    pub fn renew_credential(env: Env, engineer: Address, new_validity_period: u64) {
        ensure_not_paused(&env);
        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));
        record.issuer.require_auth();
        if !env.storage().instance().has(&trusted_key(&record.issuer)) {
            panic_with_error!(&env, ContractError::IssuerRemoved);
        }
        if !record.active {
            panic_with_error!(&env, ContractError::CredentialRevoked);
        }
        // Check if credential is hard-expired; renewal requires re-issuance
        let grace_period: u64 = env
            .storage()
            .persistent()
            .get(&GRACE_PERIOD_KEY)
            .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);
        let now = env.ledger().timestamp();
        if now >= record.expires_at + grace_period {
            panic_with_error!(&env, ContractError::CredentialExpired);
        }
        if new_validity_period < MIN_VALIDITY_PERIOD {
            panic_with_error!(&env, ContractError::InvalidValidityPeriod);
        }
        require_within_bounds(
            new_validity_period,
            MIN_VALIDITY_PERIOD,
            u64::MAX,
            "new_validity_period",
        );
        let renewed_at = env.ledger().timestamp();
        let previous_expires_at = record.expires_at;
        let renewal_base = if previous_expires_at > renewed_at {
            previous_expires_at
        } else {
            renewed_at
        };
        record.expires_at = renewal_base + new_validity_period;
        extend_persistent_ttl(&env, &engineer_key(&engineer));
        env.storage()
            .persistent()
            .set(&engineer_key(&engineer), &record);

        env.events().publish(
            (symbol_short!("RNW_CRED"), engineer.clone()),
            (
                record.issuer.clone(),
                previous_expires_at,
                record.expires_at,
                renewed_at,
            ),
        );
    }

    /// Retrieve complete engineer information by address.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer to retrieve
    ///
    /// # Returns
    /// The complete Engineer struct with all credential information
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if no engineer exists with the given address
    pub fn get_engineer(env: Env, engineer: Address) -> Engineer {
        env.storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound))
    }

    /// Get the status of an engineer's credential.
    /// Distinguishes between active, revoked, expired, and not found states.
    ///
    /// # Returns
    /// An EngineerStatus enum indicating the credential state
    pub fn get_engineer_status(env: Env, engineer: Address) -> EngineerStatus {
        match env
            .storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
        {
            Some(e) => {
                if !e.active {
                    EngineerStatus::Revoked
                } else if is_suspended(&e, env.ledger().timestamp()) {
                    EngineerStatus::Suspended
                } else if env.ledger().timestamp() >= e.expires_at {
                    EngineerStatus::Expired
                } else {
                    EngineerStatus::Active
                }
            }
            None => EngineerStatus::NotFound,
        }
    }

    /// Get the detailed credential status with grace period support.
    /// Distinguishes between valid, in grace period, hard-expired, revoked, and not found.
    /// Grace period is configurable via [`set_grace_period`] (default: 7 days).
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer to check
    ///
    /// # Returns
    /// A CredentialStatus enum with the detailed credential state
    pub fn get_credential_status(env: Env, engineer: Address) -> CredentialStatus {
        let grace_period: u64 = env
            .storage()
            .persistent()
            .get(&GRACE_PERIOD_KEY)
            .unwrap_or(DEFAULT_GRACE_PERIOD_SECS);
        match env
            .storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
        {
            Some(e) => {
                if !e.active {
                    CredentialStatus::Revoked
                } else {
                    let now = env.ledger().timestamp();
                    if is_suspended(&e, now) {
                        CredentialStatus::Suspended
                    } else if now < e.expires_at {
                        CredentialStatus::Valid
                    } else if now < e.expires_at + grace_period {
                        CredentialStatus::GracePeriod
                    } else {
                        CredentialStatus::HardExpired
                    }
                }
            }
            None => CredentialStatus::NotFound,
        }
    }

    /// Lightweight check to determine if an engineer is currently active.
    /// Returns false for unknown addresses instead of panicking.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer to check
    ///
    /// # Returns
    /// true if the engineer exists, has active credentials, and is not expired or in grace period expiry
    pub fn is_engineer_active(env: Env, engineer: Address) -> bool {
        match env
            .storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
        {
            Some(e) => {
                e.active
                    && !is_suspended(&e, env.ledger().timestamp())
                    && env.ledger().timestamp() < e.expires_at
            }
            None => false,
        }
    }

    /// Initialize the admin address for the contract.
    /// This function should be called once immediately after deployment.
    ///
    /// # Arguments
    /// * `deployer` - The address of the contract deployer; must sign this transaction.
    /// * `admin` - The address that will have administrative privileges
    ///
    /// # Panics
    /// - [`ContractError::AdminAlreadyInitialized`] if admin has already been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if deployer is not the transaction invoker
    pub fn initialize_admin(env: Env, deployer: Address, admin: Address) {
        // SDK 22: identity enforced via require_auth below
        if false {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        // Soroban SDK removed `env.invoker()`; `require_auth` enforces the
        // deployer's signature instead, matching the standard pattern.
        deployer.require_auth();
        if env.storage().instance().has(&admin_key()) {
            panic_with_error!(&env, ContractError::AdminAlreadyInitialized);
        }
        env.storage().instance().set(&admin_key(), &admin);
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_TARGET);
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("INIT_ADM")),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Get the current admin address of the contract.
    ///
    /// # Returns
    /// The address of the current administrator
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized))
    }

    /// Propose a new admin address (step 1 of 2-step transfer).
    /// Only the current admin can propose a new admin.
    ///
    /// # Arguments
    /// * `admin` - The current admin address
    /// * `new_admin` - The address to propose as the new admin
    ///
    /// # Panics
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the current admin
    /// - [`ContractError::PendingAdminAlreadyExists`] if a pending admin already exists
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) {
        let stored_admin: Address = Self::get_admin(env.clone());
        if require_admin(&admin, &stored_admin).is_err() {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        if env.storage().instance().has(&pending_admin_key()) {
            panic_with_error!(&env, ContractError::PendingAdminAlreadyExists);
        }
        env.storage()
            .instance()
            .set(&pending_admin_key(), &new_admin);
        env.storage().instance().extend_ttl(DEFAULT_TTL_LEDGERS, DEFAULT_TTL_LEDGERS);
        env.events()
            .publish((EVENT_PROP_ADMIN,), (admin.clone(), new_admin.clone()));
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("PROP_ADM")),
            (admin, env.ledger().timestamp(), new_admin),
        );
    }

    /// Accept the admin transfer (step 2 of 2-step transfer).
    /// Only the pending admin can accept and become the new admin.
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if no pending admin exists
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the pending admin
    pub fn accept_admin(env: Env) {
        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&pending_admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        pending_admin.require_auth();
        env.storage().instance().set(&admin_key(), &pending_admin);
        env.storage().instance().remove(&pending_admin_key());
        env.storage().instance().extend_ttl(DEFAULT_TTL_LEDGERS, DEFAULT_TTL_LEDGERS);
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("ADMIN_SET")),
            (pending_admin.clone(), env.ledger().timestamp()),
        );
        env.events()
            .publish((symbol_short!("ADMIN_SET"),), (pending_admin,));
    }

    /// Admin-only function to pause the contract.
    ///
    /// When paused, all state-modifying operations return [`ContractError::Paused`].
    /// Read-only functions (e.g. [`verify_engineer`], [`get_engineer`]) remain available.
    ///
    /// # Arguments
    /// * `admin` - The address that must match the stored admin
    ///
    /// # Panics
    /// - [`ContractError::UnauthorizedAdmin`] if `admin` does not match the stored admin
    pub fn pause(env: Env, admin: Address) {
        let stored_admin: Address = Self::get_admin(env.clone());
        if require_admin(&admin, &stored_admin).is_err() {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        env.storage().persistent().set(&PAUSED_KEY, &true);
        extend_persistent_ttl(&env, &PAUSED_KEY);
        env.events()
            .publish((symbol_short!("PAUSED"),), (admin.clone(),));
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("PAUSED")),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Admin-only function to unpause the contract.
    ///
    /// Resumes normal contract operation after a [`pause`] call. All state-modifying
    /// functions become available again once unpaused.
    ///
    /// # Arguments
    /// * `admin` - The address that must match the stored admin
    ///
    /// # Panics
    /// - [`ContractError::UnauthorizedAdmin`] if `admin` does not match the stored admin
    pub fn unpause(env: Env, admin: Address) {
        let stored_admin: Address = Self::get_admin(env.clone());
        if require_admin(&admin, &stored_admin).is_err() {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        env.storage().persistent().set(&PAUSED_KEY, &false);
        extend_persistent_ttl(&env, &PAUSED_KEY);
        env.events()
            .publish((symbol_short!("UNPAUSED"),), (admin.clone(),));
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("UNPAUSED")),
            (admin, env.ledger().timestamp()),
        );
    }

    /// Check if the contract is currently paused.
    ///
    /// # Returns
    /// `true` if paused; `false` otherwise
    pub fn is_paused(env: Env) -> bool {
        is_paused(&env)
    }

    /// Admin-only function to set the configurable grace period for credential renewal.
    /// After a credential expires, engineers within the grace window still show as
    /// [`CredentialStatus::GracePeriod`] rather than [`CredentialStatus::HardExpired`].
    ///
    /// # Arguments
    /// * `admin` - The current admin address
    /// * `secs` - Grace period in seconds (0 disables the grace window entirely)
    ///
    /// # Panics
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the current admin
    pub fn set_grace_period(env: Env, admin: Address, secs: u64) {
        let stored_admin: Address = Self::get_admin(env.clone());
        if require_admin(&admin, &stored_admin).is_err() {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        env.storage().persistent().set(&GRACE_PERIOD_KEY, &secs);
        extend_persistent_ttl(&env, &GRACE_PERIOD_KEY);
        env.events()
            .publish((symbol_short!("ADM_AUD"), symbol_short!("SET_GRACE")), (admin, secs));
        env.storage()
            .persistent()
            .extend_ttl(&GRACE_PERIOD_KEY, TTL_THRESHOLD, TTL_TARGET);
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("SET_GRACE")),
            (admin, secs),
        );
    }

    /// Returns the current grace period in seconds.
    /// If never set by admin, returns the default (7 days = 604_800 seconds).
    pub fn get_grace_period(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&GRACE_PERIOD_KEY)
            .unwrap_or(DEFAULT_GRACE_PERIOD_SECS)
    }

    /// Check if an issuer is in the trusted issuers list.
    ///
    /// # Arguments
    /// * `issuer` - The address of the issuer to check
    ///
    /// # Returns
    /// `true` if the issuer is trusted; `false` otherwise
    pub fn is_trusted_issuer(env: Env, issuer: Address) -> bool {
        env.storage().instance().has(&trusted_key(&issuer))
    }

    /// Get the list of all trusted issuer addresses.
    ///
    /// # Returns
    /// A Vec containing all trusted issuer addresses
    pub fn get_trusted_issuers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&issuer_list_key())
            .unwrap_or(Vec::new(&env))
    }

    /// Admin-only function to add a new trusted issuer.
    /// Only admins can modify the trusted issuers list.
    ///
    /// # Arguments
    /// * `admin` - The admin address that must match the stored admin
    /// * `issuer` - The address of the issuer to add as trusted
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the admin
    pub fn add_trusted_issuer(env: Env, admin: Address, issuer: Address) {
        ensure_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        env.storage().instance().set(&trusted_key(&issuer), &());
        let mut list: Vec<Address> = env
            .storage()
            .instance()
            .get(&issuer_list_key())
            .unwrap_or(Vec::new(&env));
        if !list.contains(issuer.clone()) {
            list.push_back(issuer.clone());
            env.storage().instance().set(&issuer_list_key(), &list);
            env.storage()
                .instance()
                .extend_ttl(TTL_THRESHOLD, TTL_TARGET);
            env.events()
                .publish((symbol_short!("ISS_ADD"), admin.clone()), (issuer.clone(),));
            env.events().publish(
                (symbol_short!("ADM_AUD"), symbol_short!("ISS_ADD")),
                (admin, env.ledger().timestamp(), issuer),
            );
        } else {
            env.storage()
                .instance()
                .extend_ttl(TTL_THRESHOLD, TTL_TARGET);
        }
    }

    /// Admin-only function to remove a trusted issuer.
    /// Only admins can modify the trusted issuers list.
    ///
    /// # Arguments
    /// * `admin` - The admin address that must match the stored admin
    /// * `issuer` - The address of the issuer to remove from trusted list
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the admin
    pub fn remove_trusted_issuer(env: Env, admin: Address, issuer: Address) {
        ensure_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }

        // Check if issuer exists before removing
        if !env.storage().instance().has(&trusted_key(&issuer)) {
            panic_with_error!(&env, ContractError::IssuerNotFound);
        }

        env.storage().instance().remove(&trusted_key(&issuer));
        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&issuer_list_key())
            .unwrap_or(Vec::new(&env));
        let mut new_list: Vec<Address> = Vec::new(&env);
        for addr in list.iter() {
            if addr != issuer {
                new_list.push_back(addr);
            }
        }
        env.storage().instance().set(&issuer_list_key(), &new_list);
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_TARGET);

        // Revoke all active engineers registered by this issuer
        let engineers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&issuer_engineers_key(&issuer))
            .unwrap_or(Vec::new(&env));
        for engineer in engineers.iter() {
            if let Some(mut record) = env
                .storage()
                .persistent()
                .get::<_, Engineer>(&engineer_key(&engineer))
            {
                if record.active {
                    record.active = false;
                    extend_persistent_ttl(&env, &engineer_key(&engineer));
                    env.storage()
                        .persistent()
                        .set(&engineer_key(&engineer), &record);
                }
            }
        }

        env.events()
            .publish((symbol_short!("ISS_RM"), admin.clone()), (issuer.clone(),));
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("ISS_RM")),
            (admin, env.ledger().timestamp(), issuer),
        );
    }

    /// Admin-only function to register a new trusted issuer.
    ///
    /// Multiple certification bodies (e.g. ASME, IEEE, NFPA) can be trusted to
    /// credential engineers. The stored admin must authorize the call.
    ///
    /// # Arguments
    /// * `issuer` - The address of the issuer to add as trusted
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if the caller is not the admin
    pub fn register_issuer(env: Env, issuer: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        Self::add_trusted_issuer(env, admin, issuer);
    }

    /// Admin-only function to revoke a trusted issuer.
    ///
    /// Removing an issuer also revokes all active engineers it credentialed and
    /// causes [`verify_engineer`] to report their credentials as revoked.
    ///
    /// # Arguments
    /// * `issuer` - The address of the issuer to remove from the trusted list
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if the caller is not the admin
    /// - [`ContractError::IssuerNotFound`] if the issuer is not currently trusted
    pub fn revoke_issuer(env: Env, issuer: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        Self::remove_trusted_issuer(env, admin, issuer);
    }

    /// Get all engineer addresses that have been credentialed by a specific issuer.
    /// This includes both active and revoked engineers (historical registry).
    ///
    /// # Arguments
    /// * `issuer` - The address of the issuer to query
    ///
    /// # Returns
    /// A Vec containing all engineer addresses credentialed by the given issuer
    pub fn get_engineers_by_issuer(env: Env, issuer: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&issuer_engineers_key(&issuer))
            .unwrap_or(Vec::new(&env))
    }

    /// Return only the active, non-expired engineer addresses credentialed by a specific issuer.
    ///
    /// Filters the full issuer → engineers list (see [`get_engineers_by_issuer`]) to include
    /// only engineers whose credentials are currently in [`EngineerStatus::Active`] state —
    /// i.e. the record exists, `active = true`, and the expiry timestamp has not been reached.
    ///
    /// This is a convenience view for issuers who need to audit their live credentialed
    /// workforce without iterating over revoked or expired entries.
    ///
    /// # Arguments
    /// * `issuer` - The address of the issuer whose active engineers should be listed
    ///
    /// # Returns
    /// A `Vec<Address>` of engineer addresses with currently active credentials (empty if none)
    pub fn get_active_engineers_by_issuer(env: Env, issuer: Address) -> Vec<Address> {
        let engineers = Self::get_engineers_by_issuer(env.clone(), issuer);
        let mut active_engineers = Vec::new(&env);
        for engineer in engineers.iter() {
            if Self::get_engineer_status(env.clone(), engineer.clone()) == EngineerStatus::Active {
                active_engineers.push_back(engineer);
            }
        }
        active_engineers
    }

    /// Return the total number of engineer addresses that have been credentialed by a specific issuer.
    ///
    /// Counts both active and revoked/expired engineers — this is a historical count of all
    /// engineers ever registered under the given issuer, not just currently active ones.
    /// Use [`get_active_engineers_by_issuer`] to query the live active count.
    ///
    /// # Arguments
    /// * `issuer` - The address of the issuer to query
    ///
    /// # Returns
    /// The total number of engineer addresses (active + inactive) credentialed by this issuer
    pub fn get_engineer_count_by_issuer(env: Env, issuer: Address) -> u32 {
        Self::get_engineers_by_issuer(env, issuer).len()
    }

    /// Temporarily suspend an engineer's credential until `until_timestamp`.
    /// Only the original issuer may suspend.
    ///
    /// Emits `SUSP_ENG` (suspension) or `UNSUSP_E` (immediate lift) events.
    ///
    /// # Arguments
    /// * `engineer`        - Address of the engineer to suspend
    /// * `until_timestamp` - Unix timestamp at which the suspension lifts automatically
    /// * `reason`          - Short human-readable reason (stored in event, not on-chain state)
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if no record exists
    /// - [`ContractError::CredentialRevoked`] if the credential is already revoked
    /// - [`ContractError::InvalidSuspensionPeriod`] if `until_timestamp` ≤ now
    pub fn suspend_engineer(
        env: Env,
        engineer: Address,
        until_timestamp: u64,
        reason: soroban_sdk::String,
    ) {
        ensure_not_paused(&env);
        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));
        record.issuer.require_auth();
        if !record.active {
            panic_with_error!(&env, ContractError::CredentialRevoked);
        }
        let now = env.ledger().timestamp();
        if until_timestamp <= now {
            panic_with_error!(&env, ContractError::InvalidSuspensionPeriod);
        }
        record.suspension_end_time = Some(until_timestamp);
        env.storage()
            .persistent()
            .extend_ttl(&engineer_key(&engineer), TTL_THRESHOLD, TTL_TARGET);
        env.storage()
            .persistent()
            .set(&engineer_key(&engineer), &record);

        env.events().publish(
            (SUSPEND_TOPIC, engineer.clone()),
            (record.issuer.clone(), until_timestamp, reason, now),
        );
    }

    /// Check whether an engineer is currently suspended.
    ///
    /// # Returns
    /// `true` if the engineer exists and is within an active suspension window; `false` otherwise.
    pub fn is_credential_suspended(env: Env, engineer: Address) -> bool {
        match env
            .storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
        {
            Some(e) => is_suspended(&e, env.ledger().timestamp()),
            None => false,
        }
    }

    /// Get the total count of registered engineers.
    ///
    /// # Returns
    /// The total number of engineers that have been registered
    pub fn get_engineer_count(env: Env) -> u32 {
        env.storage().persistent().get(&ENGINEER_COUNT).unwrap_or(0)
    }

    /// Get the total count of registered engineers as u64.
    /// Governance and analytics view for the ENG_CNT counter.
    ///
    /// # Returns
    /// The total number of engineers that have been registered, as u64
    pub fn get_total_engineer_count(env: Env) -> u64 {
        let count: u32 = env.storage().persistent().get(&ENGINEER_COUNT).unwrap_or(0);
        count as u64
    }

    /// Admin-only function to revoke credentials for multiple engineers in a single call.
    /// Reduces operational overhead when a certification body is compromised.
    ///
    /// # Arguments
    /// * `admin` - The admin address that must match the stored admin
    /// * `engineers` - Vec of engineer addresses whose credentials should be revoked
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the admin
    /// - [`ContractError::BatchRevokeTooLarge`] if engineers.len() > MAX_BATCH_REVOKE (50)
    ///
    /// Engineers that are already revoked or not found are silently skipped.
    /// A `REV_CRED` event is emitted for each successfully revoked credential.
    pub fn batch_revoke_credentials(env: Env, admin: Address, engineers: Vec<Address>) {
        ensure_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        if engineers.len() > MAX_BATCH_REVOKE {
            panic_with_error!(&env, ContractError::BatchRevokeTooLarge);
        }
        let timestamp = env.ledger().timestamp();
        for engineer in engineers.iter() {
            if let Some(mut record) = env
                .storage()
                .persistent()
                .get::<_, Engineer>(&engineer_key(&engineer))
            {
                if record.active {
                    record.active = false;
                    extend_persistent_ttl(&env, &engineer_key(&engineer));
                    env.storage().persistent().extend_ttl(
                        &engineer_key(&engineer),
                        TTL_THRESHOLD,
                        TTL_TARGET,
                    );
                    env.storage()
                        .persistent()
                        .set(&engineer_key(&engineer), &record);
                    env.events().publish(
                        (REVOKE_TOPIC, engineer.clone()),
                        (
                            engineer.clone(),
                            record.credential_hash.clone(),
                            record.issuer.clone(),
                            timestamp,
                        ),
                    );
                }
            }
        }
    }

    /// Propose a WASM upgrade for the engineer registry contract.
    /// Must be followed by `execute_upgrade` after the timelock delay.
    ///
    /// # Arguments
    /// * `admin` - The admin address that must match the stored admin
    /// * `new_wasm_hash` - The hash of the new WASM to deploy
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the admin
    pub fn propose_upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        ensure_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }

        env.storage().instance().extend_ttl(DEFAULT_TTL_LEDGERS, DEFAULT_TTL_LEDGERS);

        let tl_key = upgrade_timelock_key();
        env.storage().persistent().set(
            &tl_key,
            &TimelockProposal {
                proposed_at: env.ledger().timestamp(),
                executed: false,
            },
        );
        extend_persistent_ttl(&env, &tl_key);
        env.storage()
            .persistent()
            .set(&symbol_short!("PEND_UPG"), &new_wasm_hash);
        extend_persistent_ttl(&env, &symbol_short!("PEND_UPG"));
        env.storage().persistent().extend_ttl(
            &symbol_short!("PEND_UPG"),
            TTL_THRESHOLD,
            TTL_TARGET,
        );

        env.events().publish(
            (symbol_short!("PROP_UPG"), admin.clone()),
            (new_wasm_hash, env.ledger().timestamp()),
        );
    }

    /// Execute a previously proposed WASM upgrade after the timelock delay has expired.
    ///
    /// # Arguments
    /// * `admin` - The admin address that must match the stored admin
    ///
    /// # Panics
    /// - [`ContractError::NotInitialized`] if the admin has not been initialized
    /// - [`ContractError::UnauthorizedAdmin`] if caller is not the admin
    /// - [`ContractError::ProposalNotFound`] if no upgrade was proposed or already executed
    /// - [`ContractError::TimelockNotExpired`] if the delay has not elapsed
    pub fn execute_upgrade(env: Env, admin: Address) {
        ensure_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&admin_key())
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::NotInitialized));
        if stored_admin != admin {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }

        require_upgrade_timelock_ready(&env);

        let new_wasm_hash: BytesN<32> = env
            .storage()
            .persistent()
            .get(&symbol_short!("PEND_UPG"))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::ProposalNotFound));
        env.storage()
            .persistent()
            .remove(&symbol_short!("PEND_UPG"));

        env.storage().instance().extend_ttl(DEFAULT_TTL_LEDGERS, DEFAULT_TTL_LEDGERS);

        env.events().publish(
            (symbol_short!("UPGRADE"), admin.clone()),
            new_wasm_hash.clone(),
        );
        env.events().publish(
            (symbol_short!("ADM_AUD"), symbol_short!("UPGRADE")),
            (admin, env.ledger().timestamp(), new_wasm_hash.clone()),
        );

        #[cfg(not(test))]
        {
            env.deployer().update_current_contract_wasm(new_wasm_hash);
        }
    }

    /// Update an engineer's reputation score. Callable only by the lifecycle contract.
    /// Reputation is clamped to 0–1000.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer
    /// * `delta` - Points to add (positive) or subtract (negative)
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if the engineer record does not exist
    pub fn update_reputation(env: Env, engineer: Address, delta: i32) {
        env.current_contract_address().require_auth();
        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));
        let new_rep = (record.reputation_score as i64)
            .saturating_add(delta as i64)
            .clamp(0, 1000) as u32;
        record.reputation_score = new_rep;
        env.storage()
            .persistent()
            .set(&engineer_key(&engineer), &record);
        env.storage()
            .persistent()
            .extend_ttl(&engineer_key(&engineer), TTL_THRESHOLD, TTL_TARGET);
    }

    /// Get an engineer's current reputation score (range: 0–1000).
    ///
    /// Reputation is a weighted signal of an engineer's submission history and is used
    /// by the lifecycle contract to scale the collateral score increment:
    /// - `0` → 0.5× multiplier (new or penalised engineer)
    /// - `500` → 1.0× multiplier (neutral / default)
    /// - `1000` → 1.5× multiplier (highly reputable engineer)
    ///
    /// Returns `0` if the engineer record does not exist rather than panicking, so
    /// DeFi integrators can safely call this for any address.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer to query
    ///
    /// # Returns
    /// The engineer's reputation score in the range `[0, 1000]`, or `0` if not found
    pub fn get_reputation(env: Env, engineer: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
            .map(|e| e.reputation_score)
            .unwrap_or(0)
    }

    /// Add a specialization to an engineer's profile.
    /// Only the engineer's original issuer can modify specializations.
    ///
    /// # Arguments
    /// * `issuer` - The issuer address (must match the engineer's original issuer)
    /// * `engineer` - The address of the engineer
    /// * `specialization` - The specialization symbol (must be a valid allowed value)
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if the engineer record does not exist
    /// - [`ContractError::UntrustedIssuer`] if the issuer is not trusted
    /// - [`ContractError::UnauthorizedAdmin`] if the caller is not the engineer's original issuer
    /// - [`ContractError::InvalidSpecialization`] if the specialization is not in the allowed list
    /// - [`ContractError::SpecializationAlreadyExists`] if the engineer already has this specialization
    /// - [`ContractError::CredentialRevoked`] if the engineer's credential has been revoked
    pub fn add_specialization(
        env: Env,
        issuer: Address,
        engineer: Address,
        specialization: Symbol,
    ) {
        ensure_not_paused(&env);
        issuer.require_auth();
        if !env.storage().instance().has(&trusted_key(&issuer)) {
            panic_with_error!(&env, ContractError::UntrustedIssuer);
        }

        let allowed_specs: [Symbol; 8] = [
            symbol_short!("diesel_ge"),
            symbol_short!("wind_turb"),
            symbol_short!("solar_pnl"),
            symbol_short!("grid_infr"),
            symbol_short!("gas_turbn"),
            symbol_short!("hydroelec"),
            symbol_short!("batteryst"),
            symbol_short!("transform"),
        ];
        let mut found = false;
        for allowed in allowed_specs.iter() {
            if *allowed == specialization {
                found = true;
                break;
            }
        }
        if !found {
            panic_with_error!(&env, ContractError::InvalidSpecialization);
        }

        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));

        if record.issuer != issuer {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }
        if !record.active {
            panic_with_error!(&env, ContractError::CredentialRevoked);
        }

        for spec in record.specializations.iter() {
            if spec == specialization {
                panic_with_error!(&env, ContractError::SpecializationAlreadyExists);
            }
        }

        record.specializations.push_back(specialization.clone());
        env.storage()
            .persistent()
            .set(&engineer_key(&engineer), &record);
        env.storage()
            .persistent()
            .extend_ttl(&engineer_key(&engineer), TTL_THRESHOLD, TTL_TARGET);

        env.events().publish(
            (symbol_short!("ADD_SPEC"), engineer.clone()),
            (specialization,),
        );
    }

    /// Remove a specialization from an engineer's profile.
    /// Only the engineer's original issuer can modify specializations.
    /// Silently succeeds if the specialization does not exist.
    ///
    /// # Arguments
    /// * `issuer` - The issuer address (must match the engineer's original issuer)
    /// * `engineer` - The address of the engineer
    /// * `specialization` - The specialization symbol to remove
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if the engineer record does not exist
    /// - [`ContractError::UntrustedIssuer`] if the issuer is not trusted
    /// - [`ContractError::UnauthorizedAdmin`] if the caller is not the engineer's original issuer
    pub fn remove_specialization(
        env: Env,
        issuer: Address,
        engineer: Address,
        specialization: Symbol,
    ) {
        ensure_not_paused(&env);
        issuer.require_auth();
        if !env.storage().instance().has(&trusted_key(&issuer)) {
            panic_with_error!(&env, ContractError::UntrustedIssuer);
        }

        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound));

        if record.issuer != issuer {
            panic_with_error!(&env, ContractError::UnauthorizedAdmin);
        }

        let mut new_specs: Vec<Symbol> = Vec::new(&env);
        let mut found = false;
        for spec in record.specializations.iter() {
            if spec == specialization {
                found = true;
            } else {
                new_specs.push_back(spec);
            }
        }

        if found {
            record.specializations = new_specs;
            env.storage()
                .persistent()
                .set(&engineer_key(&engineer), &record);
            env.storage()
                .persistent()
                .extend_ttl(&engineer_key(&engineer), TTL_THRESHOLD, TTL_TARGET);

            env.events().publish(
                (symbol_short!("RM_SPEC"), engineer.clone()),
                (specialization,),
            );
        }
    }

    /// Get the list of specializations for an engineer.
    /// Returns an empty Vec if the engineer has no specializations.
    ///
    /// # Arguments
    /// * `engineer` - The address of the engineer
    ///
    /// # Returns
    /// A Vec of specialization symbols
    ///
    /// # Panics
    /// - [`ContractError::EngineerNotFound`] if the engineer record does not exist
    pub fn get_specializations(env: Env, engineer: Address) -> Vec<Symbol> {
        env.storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
            .unwrap_or_else(|| panic_with_error!(&env, ContractError::EngineerNotFound))
            .specializations
    }
}
