# Engineer Credentialing System

This document describes the federated credentialing system used in Mainstay to verify and manage engineer identities and qualifications for maintenance operations.

## Overview

The engineer credentialing system provides a decentralized, trustless way to verify that only qualified engineers can sign maintenance records on industrial assets. It uses a federated model where trusted issuers can credential engineers, creating a verifiable chain of trust.

## System Architecture

### Key Components

1. **Trusted Issuers** - Organizations authorized to issue credentials
2. **Engineer Credentials** - Cryptographic proofs of engineer qualifications  
3. **Verification System** - On-chain validation of credential status
4. **Revocation Mechanism** - Ability to invalidate compromised credentials

## Credential Lifecycle

### 1. Issuance
- **Who**: Trusted issuer organizations
- **What**: Engineer addresses with credential hashes
- **Validation**: Issuer must be in trusted issuers list
- **Security**: Zero-hash credentials are rejected

### 2. Verification
- **Who**: Anyone can verify
- **What**: Checks both active status and expiration
- **Result**: Boolean indicating current validity
- **Use Case**: Maintenance contract validation

### 3. Revocation
- **Who**: Original issuing authority only
- **What**: Deactivates credential (sets active=false)
- **Persistence**: Record remains for audit trail
- **Security**: Prevents unauthorized revocation
- **Events**: Each revoked credential emits its own `REV_CRED` event, including when
  revoked in bulk via `batch_revoke_credentials` (see [Event Reference](#event-reference))

## Data Structures

### Engineer Record
```rust
pub struct Engineer {
    pub address: Address,           // Engineer's wallet address
    pub credential_hash: BytesN<32>, // Hash of qualifications/certs
    pub issuer: Address,            // Who issued the credential
    pub active: bool,               // Current validity status
    pub issued_at: u64,            // When credential was issued
    pub expires_at: u64,            // When credential expires
}
```

### Credential Hash
- **Purpose**: Cryptographic fingerprint of engineer qualifications
- **Content**: Typically hash of certificates, licenses, training records
- **Security**: Prevents credential tampering and forgery
- **Format**: 32-byte SHA-256 hash

#### Contract Requirement: `credential_hash == sha256(credential_data)`

The `credential_hash` passed to `register_engineer` **MUST** be the SHA-256 digest of the
canonical credential data for that engineer. The contract stores the hash as an opaque
`BytesN<32>` and cannot recompute it on-chain, so it enforces only the zero-hash guard
(see [Zero-Hash Protection](#zero-hash-protection)). Correctness of the hash therefore
depends on the issuer computing it exactly as specified below.

- **Definition**: `credential_hash = sha256(credential_data)`
- **`credential_data`**: the canonical, deterministic serialization of the engineer's
  credential payload (see [Canonical Credential Data](#canonical-credential-data)).
- **Encoding**: raw 32-byte SHA-256 digest, no hex prefix, no truncation, no padding.
- **Prohibited values**: all-zeros, the SHA-256 of an empty string
  (`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`), or any hash
  not derived from real credential data. These are unverifiable off-chain and are
  treated as invalid credentials by verifiers.

> **Note:** The contract cannot validate that a non-zero hash corresponds to real
> credential data. Issuers are responsible for computing the hash correctly, and
> verifiers are responsible for recomputing it off-chain (see
> [Off-Chain Verification](#off-chain-verification)).

#### Canonical Credential Data

To make hashes reproducible across issuers and verifiers, `credential_data` must be a
deterministic byte string. The recommended canonical form is the UTF-8 encoding of a
JSON object with sorted keys and no insignificant whitespace, for example:

```json
{"engineer":"G...ADDRESS","issuer":"G...ADDRESS","licenses":["LIC-123"],"name":"Jane Doe","valid_from":1700000000}
```

Any change to the payload (including key order or whitespace) changes the hash, so the
exact serialization used at issuance must be published alongside the credential.

## Trusted Issuer Model

### Issuer Registration
- **Authority**: Contract administrators only
- **Validation**: Address verification and authorization
- **Storage**: Instance storage for global access
- **Listing**: Maintained in trusted issuers vector

### Issuer Responsibilities
- **Vetting**: Verify engineer qualifications before issuing
- **Standards**: Follow consistent credentialing standards
- **Security**: Protect issuer private keys
- **Compliance**: Follow regulatory requirements
- **Hashing**: Compute `credential_hash = sha256(credential_data)` exactly as specified
  above and publish the canonical `credential_data` so verifiers can recompute it

### Issuer Benefits
- **Reputation**: Build trusted brand in ecosystem
- **Revenue**: Potential credentialing service fees
- **Network**: Connect with qualified engineers
- **Authority**: Participate in governance

## Security Features

### Zero-Hash Protection
```rust
if credential_hash == BytesN::from_array(&env, &[0u8; 32]) {
    panic_with_error!(&env, ContractError::InvalidCredentialHash);
}
```

### Issuer Authorization
- **Registration**: Admin-only function to add issuers
- **Verification**: Only trusted issuers can credential engineers
- **Removal**: Admin-only function to remove issuers
- **Audit Trail**: All changes emit events

### Expiration Handling
- **Automatic**: Credentials expire based on validity period
- **Verification**: Expired credentials return false
- **Flexibility**: Validity period set per credential
- **Renewal**: New credentials issued after expiration

## API Operations

### For Engineers
```rust
// Check if your credential is valid
verify_engineer(your_address) -> bool

// Get your credential details
get_engineer(your_address) -> Engineer

// Find engineers credentialed by same issuer
get_engineers_by_issuer(issuer_address) -> Vec<Address>
```

### For Issuers
```rust
// Register a new engineer
// credential_hash MUST equal sha256(credential_data); see "Credential Hash" above.
register_engineer(
    engineer_address,
    credential_hash,
    issuer_address,
    validity_period_seconds
)

// Revoke a credential
revoke_credential(engineer_address)

// Revoke multiple credentials in one call
// Emits one REV_CRED event per engineer (see Event Reference)
batch_revoke_credentials(engineer_addresses)

// Check if you're a trusted issuer
is_trusted_issuer(your_address) -> bool
```

### For Administrators
```rust
// Add a trusted issuer
add_trusted_issuer(admin_address, issuer_address)

// Remove a trusted issuer  
remove_trusted_issuer(admin_address, issuer_address)

// Get all trusted issuers
get_trusted_issuers() -> Vec<Address>
```

## Event Reference

All credential state changes emit events so off-chain indexers can reconstruct the full
audit trail. Indexers should subscribe to individual events rather than relying on
summary events, since bulk operations emit one event per affected engineer.

### `REV_CRED` — Credential Revoked

Emitted once for **each** credential that is revoked, whether via `revoke_credential`
(single) or `batch_revoke_credentials` (bulk). A batch of N revocations therefore emits
N `REV_CRED` events, one per engineer, so indexers that listen for individual
revocations never miss a batch revocation.

- **Topics**: `("REV_CRED", engineer_address)`
- **Data**: the revoked engineer's address
- **Emission points**:
  - `revoke_credential(engineer_address)` — one event for the single engineer
  - `batch_revoke_credentials(engineer_addresses)` — one event per engineer inside the
    revocation loop (N events for N revocations)

> **Note:** `batch_revoke_credentials` does not emit a single summary event in place of
> the per-engineer events. Each revocation in the batch produces its own `REV_CRED`
> event, preserving a complete and unambiguous audit trail.

## Off-Chain Verification

Because the contract stores `credential_hash` as an opaque `BytesN<32>`, verifiers must
recompute the hash from the credential data to confirm it matches what was registered.

### Verification Procedure

1. **Fetch the on-chain record**: call `get_engineer(address)` and read
   `credential_hash`, `issuer`, `issued_at`, and `expires_at`.
2. **Obtain the credential data**: retrieve the canonical `credential_data` published by
   the issuer for that engineer (the exact bytes used at issuance).
3. **Recompute the hash**: compute `sha256(credential_data)` using the same canonical
   serialization described in [Canonical Credential Data](#canonical-credential-data).
4. **Compare**: the recomputed 32-byte digest must equal the on-chain `credential_hash`
   byte-for-byte. If they differ, the credential is unverifiable and must be rejected.
5. **Check status**: also confirm `active == true` and `expires_at` is in the future
   (or call `verify_engineer(address)`).

### Reference Implementation (JavaScript)

```js
import { createHash } from "crypto";

// credentialData must be the exact canonical bytes used at issuance.
function credentialHash(credentialData) {
  return createHash("sha256").update(credentialData).digest(); // 32-byte Buffer
}

function verifyCredential(onChainHashHex, credentialData) {
  const recomputed = credentialHash(credentialData);
  return recomputed.toString("hex") === onChainHashHex;
}
```
