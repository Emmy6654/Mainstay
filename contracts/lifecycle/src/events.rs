//! Event symbol constants for the Lifecycle contract.
//!
//! Centralising all `EVENT_*` constants here makes it straightforward to audit
//! which on-chain events the contract emits and ensures consistent naming
//! across the codebase.

use soroban_sdk::{symbol_short, Symbol};

/// Emitted once when the contract is first initialised.
pub(crate) const EVENT_INIT: Symbol = symbol_short!("INIT");

/// Emitted on every successful `submit_maintenance` call.
pub(crate) const EVENT_MAINT: Symbol = symbol_short!("MAINT");

/// Emitted when a score-decay step is applied to an asset.
pub(crate) const EVENT_DECAY: Symbol = symbol_short!("DECAY");

/// Emitted when an asset is registered in the asset registry cross-contract call.
pub(crate) const EVENT_REG_AST: Symbol = symbol_short!("REG_AST");

/// Emitted when an engineer is registered in the engineer registry cross-contract call.
pub(crate) const EVENT_REG_ENG: Symbol = symbol_short!("REG_ENG");

/// Emitted when an asset's collateral score is reset by an admin.
pub(crate) const EVENT_RST_SCR: Symbol = symbol_short!("RST_SCR");

/// Emitted when an asset ownership transfer sentinel is written.
pub(crate) const EVENT_XFER: Symbol = symbol_short!("XFER");

/// Emitted when a new admin is proposed (step 1 of the 2-step admin transfer).
pub(crate) const EVENT_PROP_ADMIN: Symbol = symbol_short!("PROP_ADM");

/// Emitted when a pending admin accepts and becomes the active admin.
pub(crate) const EVENT_ADMIN_SET: Symbol = symbol_short!("ADMIN_SET");

/// Emitted when a maintenance history or score history is pruned by an admin.
pub(crate) const EVENT_PRUNED: Symbol = symbol_short!("PRUNED");

/// Emitted when an admin proposes a new task-type weight via `propose_weight_change`.
pub(crate) const EVENT_WEIGHT_PROP: Symbol = symbol_short!("WT_PROP");

/// Emitted when an admin executes a pending weight-change proposal via `execute_weight_change`.
pub(crate) const EVENT_WEIGHT_EXEC: Symbol = symbol_short!("WT_EXEC");

/// Emitted when an admin anchors maintenance history to a health snapshot via `anchor_history_to_snapshot`.
pub(crate) const EVENT_RECONSTR: Symbol = symbol_short!("RECONSTR");

/// Emitted when asset retirement is initiated via `initiate_retirement`.
pub(crate) const EVENT_RETIREMENT_INIT: Symbol = symbol_short!("RTR_INIT");

/// Emitted when asset retirement is confirmed via `confirm_retirement`.
pub(crate) const EVENT_RETIREMENT_CONF: Symbol = symbol_short!("RTR_CONF");

/// Emitted when asset retirement is cancelled via `cancel_retirement`.
pub(crate) const EVENT_RETIREMENT_CANC: Symbol = symbol_short!("RTR_CANC");

/// Emitted when a coordinated maintenance task is created via `create_coordinated_task`.
pub(crate) const EVENT_COORD_TASK_CREATE: Symbol = symbol_short!("COOD_CRT");

/// Emitted when a coordinated task is completed via `complete_coordinated_task`.
pub(crate) const EVENT_COORD_TASK_DONE: Symbol = symbol_short!("COOD_DON");

/// Emitted when seasonal adjustment factors are configured via `set_seasonal_adjustment`.
pub(crate) const EVENT_SEASONAL_ADJ: Symbol = symbol_short!("SEAS_SET");
