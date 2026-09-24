#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec};

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

fn engineer_key(addr: &Address) -> (Symbol, Address) {
    (symbol_short!("ENG"), addr.clone())
}

fn parent_key(spec: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("PARENT"), spec.clone())
}

fn spec_index_key(spec: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("SPECIDX"), spec.clone())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, BytesN, Env};

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
}
