//! Storage key helpers for the Lifecycle contract.
//!
//! All persistent-storage keys are defined here so that key construction is
//! centralised and consistent across the codebase.  Every key-returning
//! function is `pub(crate)` so it can be called from `lib.rs` and `admin.rs`
//! without being part of the public contract ABI.

use soroban_sdk::{symbol_short, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Per-asset keys
// ---------------------------------------------------------------------------

/// Maintenance history for an asset: `Vec<MaintenanceRecord>`.
pub(crate) fn history_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("HIST"), asset_id)
}

/// Current collateral score for an asset: `u32`.
pub(crate) fn score_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("SCORE"), asset_id)
}

/// Score history (time-series) for an asset: `Vec<ScoreEntry>`.
pub(crate) fn score_history_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("SCHIST"), asset_id)
}

/// Timestamp of the last score update for an asset: `u64`.
pub(crate) fn last_update_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("LUPD"), asset_id)
}

/// Frozen flag for an asset: `bool`.
pub(crate) fn frozen_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("FROZEN"), asset_id)
}

/// Frozen score for an asset (score captured at freeze time): `u32`.
///
/// TTL strategy: this key's TTL is extended both when it is written (freeze
/// time) and on every subsequent read (`get_collateral_score_opt`,
/// `batch_is_collateral_eligible`). A frozen asset can otherwise go
/// unqueried for long stretches; extending only on write would let the TTL
/// lapse and silently drop the stored value, causing reads to fall back to
/// `0` instead of the preserved frozen score.
pub(crate) fn frozen_score_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("FRZ_SCR"), asset_id)
}

/// Health-snapshot list for an asset: `Vec<HealthSnapshot>`.
pub(crate) fn health_snapshot_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("HLTH_SNP"), asset_id)
}

/// Ownership-transfer history for an asset: `Vec<TransferRecord>`.
pub(crate) fn transfer_hist_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("XFER_HIST"), asset_id)
}

// ---------------------------------------------------------------------------
// Per-engineer keys
// ---------------------------------------------------------------------------

/// List of asset IDs an engineer has worked on: `Vec<u64>`.
pub(crate) fn engineer_history_key(engineer: &Address) -> (Symbol, Address) {
    (symbol_short!("ENG_HIST"), engineer.clone())
}

/// Per-asset authorization flag for an engineer: `bool`.
pub(crate) fn engineer_auth_key(asset_id: u64, engineer: &Address) -> (Symbol, u64, Address) {
    (symbol_short!("ENG_AUTH"), asset_id, engineer.clone())
}

/// Rolling-hour submission rate window for an engineer: `(window_start: u64, count: u32)`.
pub(crate) fn submission_window_key(engineer: &Address) -> (Symbol, Address) {
    (symbol_short!("SUB_WIN"), engineer.clone())
}

/// Timelock proposal for revoking an engineer's auth on an asset: `TimelockProposal`.
pub(crate) fn revoke_eng_timelock_key(
    asset_id: u64,
    engineer: &Address,
) -> (Symbol, u64, Address) {
    (symbol_short!("RVK_TL"), asset_id, engineer.clone())
}

// ---------------------------------------------------------------------------
// Timelock keys
// ---------------------------------------------------------------------------

/// Generic timelock proposal key, keyed by operation symbol: `TimelockProposal`.
pub(crate) fn timelock_key(op: Symbol) -> (Symbol, Symbol) {
    (symbol_short!("TL_PROP"), op)
}

// ---------------------------------------------------------------------------
// Asset-type keys
// ---------------------------------------------------------------------------

/// Maintenance standard bytes for an asset type: `Bytes`.
pub(crate) fn standard_key(asset_type: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("MSTD"), asset_type.clone())
}

/// Dynamic frequency scoring weights JSON for an asset type: `Bytes`.
///
/// `env` is required to construct the `Symbol` used as a discriminant; it is
/// not stored itself.
pub(crate) fn scoring_weights_key(_env: &Env, asset_type: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("SCR_WGT"), asset_type.clone())
}

// ---------------------------------------------------------------------------
// Cross-Contract Score Consensus keys (Issue #1637)
// ---------------------------------------------------------------------------

/// External scores submitted by providers for an asset: `Vec<ExternalScoreEntry>`.
pub(crate) fn external_scores_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("EXT_SCR"), asset_id)
}

/// List of authorized score providers: `Vec<Address>`.
pub(crate) fn score_providers_key() -> Symbol {
    symbol_short!("PROVIDERS")
}

/// Provider reputation scores: `Map<Address, u32>`.
pub(crate) fn provider_reputation_key() -> Symbol {
    symbol_short!("PROV_REP")
}

// ---------------------------------------------------------------------------
// Score Anomaly Detection keys (Issue #1639)
// ---------------------------------------------------------------------------

/// Baseline scores for anomaly detection: `Vec<u64>`.
pub(crate) fn score_baseline_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("BASE_SCR"), asset_id)
}

/// Detected anomalies for an asset: `Vec<ScoreAnomaly>`.
pub(crate) fn score_anomalies_key(asset_id: u64) -> (Symbol, u64) {
    (symbol_short!("ANOM_SCR"), asset_id)
}

// ---------------------------------------------------------------------------
// Score Degradation Curves keys (Issue #1638)
// ---------------------------------------------------------------------------

/// Degradation curve for an asset category: `DegradationCurve`.
pub(crate) fn degradation_curve_key(category: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("DEG_CURVE"), category.clone())
}

/// All degradation curves: `Map<Symbol, DegradationCurve>`.
pub(crate) fn degradation_curves_key() -> Symbol {
    symbol_short!("ALL_CURVES")
}

// ---------------------------------------------------------------------------
// Peer Comparison Scoring keys (Issue #1640)
// ---------------------------------------------------------------------------

/// Peer group statistics: `PeerGroup`.
pub(crate) fn peer_group_key(group_id: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("PEER_GRP"), group_id.clone())
}
