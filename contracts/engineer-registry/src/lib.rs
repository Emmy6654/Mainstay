#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone)]
pub struct Engineer {
    pub address: Address,
    pub credential_hash: BytesN<32>,
    pub issuer: Address,
    pub active: bool,
    pub issued_at: u64,
    pub specialization: Symbol,
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReviewVisibility {
    Public,
    Private,
    PeersOnly,
}

#[contracttype]
#[derive(Clone)]
pub struct Review {
    pub reviewer: Address,
    pub engineer: Address,
    pub rating: u32,
    pub feedback: Bytes,
    pub visibility: ReviewVisibility,
    pub disputed: bool,
    pub submitted_at: u64,
}

fn engineer_key(addr: &Address) -> (Symbol, Address) {
    (symbol_short!("ENG"), addr.clone())
}

fn parent_key(spec: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("PARENT"), spec.clone())
}

fn spec_index_key(spec: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("SPECIDX"), spec.clone())
}

fn reviewer_key(addr: &Address) -> (Symbol, Address) {
    (symbol_short!("REVIEWER"), addr.clone())
}

fn review_key(engineer: &Address, reviewer: &Address) -> (Symbol, Address, Address) {
    (symbol_short!("REVIEW"), engineer.clone(), reviewer.clone())
}

fn reviews_key(engineer: &Address) -> (Symbol, Address) {
    (symbol_short!("REVIEWS"), engineer.clone())
}

fn rating_key(engineer: &Address) -> (Symbol, Address) {
    (symbol_short!("RATING"), engineer.clone())
}

fn reputation_key(addr: &Address) -> (Symbol, Address) {
    (symbol_short!("REPUT"), addr.clone())
}

#[contract]
pub struct EngineerRegistry;

#[contractimpl]
impl EngineerRegistry {
    pub fn register_engineer(
        env: Env,
        engineer: Address,
        credential_hash: BytesN<32>,
        issuer: Address,
    ) {
        issuer.require_auth();
        assert!(credential_hash != BytesN::from_array(&env, &[0u8; 32]), "credential hash cannot be zero");
        let record = Engineer {
            address: engineer.clone(),
            credential_hash,
            issuer,
            active: true,
            issued_at: env.ledger().timestamp(),
            specialization: symbol_short!("GENERAL"),
        };
        env.storage().persistent().set(&engineer_key(&engineer), &record);
    }

    pub fn verify_engineer(env: Env, engineer: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, Engineer>(&engineer_key(&engineer))
            .map(|e| e.active)
            .unwrap_or(false)
    }

    pub fn revoke_credential(env: Env, engineer: Address, issuer: Address) {
        issuer.require_auth();
        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .expect("engineer not found");
        assert!(record.issuer == issuer, "not the issuer");
        record.active = false;
        env.storage().persistent().set(&engineer_key(&engineer), &record);
    }

    pub fn get_engineer(env: Env, engineer: Address) -> Engineer {
        env.storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .expect("engineer not found")
    }

    pub fn add_specialization_hierarchy(env: Env, parent: Symbol, child: Symbol) {
        assert!(parent != child, "parent and child must differ");
        env.storage().persistent().set(&parent_key(&child), &parent);
    }

    pub fn get_specialization_parent(env: Env, spec: Symbol) -> Option<Symbol> {
        env.storage().persistent().get(&parent_key(&spec))
    }

    pub fn set_engineer_specialization(env: Env, engineer: Address, spec: Symbol) {
        let mut record: Engineer = env
            .storage()
            .persistent()
            .get(&engineer_key(&engineer))
            .expect("engineer not found");
        record.specialization = spec.clone();
        env.storage().persistent().set(&engineer_key(&engineer), &record);

        let mut index: Vec<Address> = env
            .storage()
            .persistent()
            .get(&spec_index_key(&spec))
            .unwrap_or(Vec::new(&env));
        if !index.contains(&engineer) {
            index.push_back(engineer);
            env.storage().persistent().set(&spec_index_key(&spec), &index);
        }
    }

    pub fn get_applicable_engineers(env: Env, required_spec: Symbol) -> Vec<Address> {
        let mut result: Vec<Address> = Vec::new(&env);
        let mut current = Some(required_spec);
        while let Some(spec) = current {
            let index: Vec<Address> = env
                .storage()
                .persistent()
                .get(&spec_index_key(&spec))
                .unwrap_or(Vec::new(&env));
            for addr in index.iter() {
                if let Some(record) = env
                    .storage()
                    .persistent()
                    .get::<_, Engineer>(&engineer_key(&addr))
                {
                    if record.active && !result.contains(&addr) {
                        result.push_back(addr);
                    }
                }
            }
            current = env.storage().persistent().get(&parent_key(&spec));
        }
        result
    }

    pub fn authorize_reviewer(env: Env, reviewer: Address) {
        reviewer.require_auth();
        env.storage().persistent().set(&reviewer_key(&reviewer), &true);
    }

    pub fn is_authorized_reviewer(env: Env, reviewer: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&reviewer_key(&reviewer))
            .unwrap_or(false)
    }

    pub fn submit_engineer_review(
        env: Env,
        reviewer: Address,
        engineer: Address,
        rating: u32,
        feedback: Bytes,
        visibility: ReviewVisibility,
    ) {
        reviewer.require_auth();
        assert!(rating >= 1 && rating <= 5, "rating must be between 1 and 5");
        assert!(reviewer != engineer, "cannot review yourself");
        assert!(
            env.storage()
                .persistent()
                .get::<_, bool>(&reviewer_key(&reviewer))
                .unwrap_or(false),
            "reviewer not authorized"
        );
        assert!(
            env.storage()
                .persistent()
                .has(&engineer_key(&engineer)),
            "engineer not found"
        );

        let review = Review {
            reviewer: reviewer.clone(),
            engineer: engineer.clone(),
            rating,
            feedback,
            visibility,
            disputed: false,
            submitted_at: env.ledger().timestamp(),
        };
        env.storage()
            .persistent()
            .set(&review_key(&engineer, &reviewer), &review);

        let mut reviewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&reviews_key(&engineer))
            .unwrap_or(Vec::new(&env));
        if !reviewers.contains(&reviewer) {
            reviewers.push_back(reviewer.clone());
            env.storage()
                .persistent()
                .set(&reviews_key(&engineer), &reviewers);
        }

        let reputation: u32 = env
            .storage()
            .persistent()
            .get(&reputation_key(&reviewer))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&reputation_key(&reviewer), &(reputation + 1));
    }

    pub fn get_engineer_rating(env: Env, engineer: Address) -> (u32, u32) {
        let reviewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&reviews_key(&engineer))
            .unwrap_or(Vec::new(&env));
        let mut total: u32 = 0;
        let mut count: u32 = 0;
        for reviewer in reviewers.iter() {
            if let Some(review) = env
                .storage()
                .persistent()
                .get::<_, Review>(&review_key(&engineer, &reviewer))
            {
                if !review.disputed {
                    total += review.rating;
                    count += 1;
                }
            }
        }
        let average = if count == 0 { 0 } else { total / count };
        (average, count)
    }

    pub fn get_engineer_review(
        env: Env,
        engineer: Address,
        reviewer: Address,
    ) -> Option<Review> {
        env.storage()
            .persistent()
            .get(&review_key(&engineer, &reviewer))
    }

    pub fn get_visible_reviews(
        env: Env,
        engineer: Address,
        viewer: Address,
    ) -> Vec<Review> {
        let reviewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&reviews_key(&engineer))
            .unwrap_or(Vec::new(&env));
        let viewer_is_peer = env
            .storage()
            .persistent()
            .has(&engineer_key(&viewer));
        let mut result: Vec<Review> = Vec::new(&env);
        for reviewer in reviewers.iter() {
            if let Some(review) = env
                .storage()
                .persistent()
                .get::<_, Review>(&review_key(&engineer, &reviewer))
            {
                let visible = match review.visibility {
                    ReviewVisibility::Public => true,
                    ReviewVisibility::Private => review.reviewer == viewer,
                    ReviewVisibility::PeersOnly => viewer_is_peer,
                };
                if visible {
                    result.push_back(review);
                }
            }
        }
        result
    }

    pub fn dispute_review(env: Env, engineer: Address, reviewer: Address) {
        engineer.require_auth();
        let mut review: Review = env
            .storage()
            .persistent()
            .get(&review_key(&engineer, &reviewer))
            .expect("review not found");
        assert!(!review.disputed, "review already disputed");
        review.disputed = true;
        env.storage()
            .persistent()
            .set(&review_key(&engineer, &reviewer), &review);

        let reputation: u32 = env
            .storage()
            .persistent()
            .get(&reputation_key(&reviewer))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&reputation_key(&reviewer), &reputation.saturating_sub(1));
    }

    pub fn get_reviewer_reputation(env: Env, reviewer: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&reputation_key(&reviewer))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Bytes, BytesN, Env};

    #[test]
    fn test_register_verify_revoke() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EngineerRegistry, ());
        let client = EngineerRegistryClient::new(&env, &contract_id);

        let engineer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);

        client.register_engineer(&engineer, &hash, &issuer);
        assert!(client.verify_engineer(&engineer));

        client.revoke_credential(&engineer, &issuer);
        assert!(!client.verify_engineer(&engineer));
    }

    #[test]
    #[should_panic(expected = "credential hash cannot be zero")]
    fn test_register_zero_hash_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EngineerRegistry, ());
        let client = EngineerRegistryClient::new(&env, &contract_id);

        let engineer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

        client.register_engineer(&engineer, &zero_hash, &issuer);
    }

    #[test]
    fn test_specialization_hierarchy_and_matching() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EngineerRegistry, ());
        let client = EngineerRegistryClient::new(&env, &contract_id);

        let hvac = symbol_short!("HVAC");
        let heating = symbol_short!("HEATING");
        let cooling = symbol_short!("COOLING");
        client.add_specialization_hierarchy(&hvac, &heating);
        client.add_specialization_hierarchy(&hvac, &cooling);

        let engineer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.register_engineer(&engineer, &hash, &issuer);
        client.set_engineer_specialization(&engineer, &heating);

        let applicable = client.get_applicable_engineers(&hvac);
        assert!(applicable.contains(&engineer));

        let cooling_only = client.get_applicable_engineers(&cooling);
        assert!(!cooling_only.contains(&engineer));
    }

    #[test]
    fn test_submit_and_get_rating() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EngineerRegistry, ());
        let client = EngineerRegistryClient::new(&env, &contract_id);

        let engineer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let reviewer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.register_engineer(&engineer, &hash, &issuer);
        client.authorize_reviewer(&reviewer);

        let feedback = Bytes::from_array(&env, &[1u8, 2u8]);
        client.submit_engineer_review(
            &reviewer,
            &engineer,
            &4,
            &feedback,
            &ReviewVisibility::Public,
        );

        let (average, count) = client.get_engineer_rating(&engineer);
        assert_eq!(average, 4);
        assert_eq!(count, 1);
        assert_eq!(client.get_reviewer_reputation(&reviewer), 1);
    }

    #[test]
    fn test_dispute_review_excludes_from_rating() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EngineerRegistry, ());
        let client = EngineerRegistryClient::new(&env, &contract_id);

        let engineer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let reviewer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.register_engineer(&engineer, &hash, &issuer);
        client.authorize_reviewer(&reviewer);

        let feedback = Bytes::from_array(&env, &[1u8, 2u8]);
        client.submit_engineer_review(
            &reviewer,
            &engineer,
            &5,
            &feedback,
            &ReviewVisibility::PeersOnly,
        );
        client.dispute_review(&engineer, &reviewer);

        let (average, count) = client.get_engineer_rating(&engineer);
        assert_eq!(average, 0);
        assert_eq!(count, 0);
        assert_eq!(client.get_reviewer_reputation(&reviewer), 0);
    }

    #[test]
    #[should_panic(expected = "reviewer not authorized")]
    fn test_unauthorized_reviewer_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(EngineerRegistry, ());
        let client = EngineerRegistryClient::new(&env, &contract_id);

        let engineer = Address::generate(&env);
        let issuer = Address::generate(&env);
        let reviewer = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.register_engineer(&engineer, &hash, &issuer);

        let feedback = Bytes::from_array(&env, &[1u8, 2u8]);
        client.submit_engineer_review(
            &reviewer,
            &engineer,
            &3,
            &feedback,
            &ReviewVisibility::Public,
        );
    }
}
