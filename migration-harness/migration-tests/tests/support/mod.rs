use soroban_sdk::{contracttype, Address, Env, Symbol};

#[derive(Clone)]
#[contracttype]
enum MigrationDataKey {
    MigrationEpochStart,
    VerifiedQueueUnlock,
    ActivatedAt,
    ScheduledBackfill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationStatus {
    Pending,
    Open,
    Active,
}

#[allow(dead_code)]
pub struct MigrationState {
    pub activated_at: Option<u64>,
    pub funded_backfill: Option<i128>,
    pub migration_epoch_start: Option<u64>,
    pub scheduled_backfill: i128,
    pub status: MigrationStatus,
    pub verified_queue_unlock: Option<u64>,
}

pub fn migration_state(env: &Env, candidate: &Address) -> MigrationState {
    env.as_contract(candidate, || {
        let migration_epoch_start = env
            .storage()
            .instance()
            .get(&MigrationDataKey::MigrationEpochStart);
        let activated_at = env.storage().instance().get(&MigrationDataKey::ActivatedAt);
        MigrationState {
            activated_at,
            funded_backfill: env.storage().instance().get(&Symbol::new(env, "BFundAmt")),
            migration_epoch_start,
            scheduled_backfill: env
                .storage()
                .instance()
                .get(&MigrationDataKey::ScheduledBackfill)
                .unwrap_or(0),
            status: if activated_at.is_some() {
                MigrationStatus::Active
            } else if migration_epoch_start.is_some() {
                MigrationStatus::Open
            } else {
                MigrationStatus::Pending
            },
            verified_queue_unlock: env
                .storage()
                .instance()
                .get(&MigrationDataKey::VerifiedQueueUnlock),
        }
    })
}
