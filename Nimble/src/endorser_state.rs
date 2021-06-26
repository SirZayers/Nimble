use crate::errors::EndorserError;
use crate::helper::{concat_bytes, hash};
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer};
use rand::rngs::OsRng;
use sha3::digest::Output;
use sha3::{Digest, Sha3_256};
use std::collections::HashMap;

pub struct Store {
  pub state: EndorserState,
}

/// Public identity of the endorser
#[derive(Debug, Copy, Clone)]
pub struct EndorserIdentity {
  pubkey: PublicKey,
  sign: Signature,
}

/// Endorser's internal state
pub struct EndorserState {
  /// a key pair in the ed25519 digital signature scheme
  keypair: Keypair,

  /// a map from fixed-sized labels to a tail hash
  ledgers: HashMap<Vec<u8>, Vec<u8>>,

  /// Endorser identity
  id: EndorserIdentity,
}

impl EndorserIdentity {
  pub fn new(public_key: PublicKey, signature: Signature) -> Self {
    EndorserIdentity {
      pubkey: public_key,
      sign: signature,
    }
  }

  pub fn get_public_key(&self) -> Vec<u8> {
    self.pubkey.to_bytes().to_vec()
  }

  pub fn get_signature(&self) -> Vec<u8> {
    self.sign.to_bytes().to_vec()
  }
}

impl EndorserState {
  pub fn new() -> Self {
    let mut csprng = OsRng {};
    let keypair = Keypair::generate(&mut csprng);
    let public_key = keypair.public;
    let signature_on_public_key = keypair.sign(public_key.as_bytes());
    println!("Creating new EndorserState with a KeyPair and Signing the Public Key");
    println!("PK : {:?}", hex::encode(public_key.to_bytes()));
    EndorserState {
      keypair,
      ledgers: HashMap::new(),
      id: EndorserIdentity::new(public_key, signature_on_public_key),
    }
  }

  pub fn create_ledger(
    &mut self,
    genesis_block_hash: Vec<u8>,
  ) -> Result<(Vec<u8>, Signature, PublicKey), EndorserError> {
    // The first time a ledger is requested, generate a handle and sign it
    let handle = genesis_block_hash.clone();
    let sign_handle = self.keypair.sign(handle.as_slice());
    let sign_hash = hash(&sign_handle.clone().to_bytes());
    println!("Inserting {:?} --> {:?}", handle, sign_hash);
    self.ledgers.insert(handle.clone(), sign_hash.to_vec());
    Ok((handle.clone(), sign_handle, self.keypair.public))
  }

  pub fn get_handles_in_endorser_state(&self) -> Result<Vec<Vec<u8>>, EndorserError> {
    let handles = self.ledgers.keys().cloned().collect();
    Ok(handles)
  }

  pub fn get_tail(&self, endorser_handle: Vec<u8>) -> Result<Vec<u8>, EndorserError> {
    println!("Handle: {:?}", endorser_handle);
    if self.ledgers.contains_key(&*endorser_handle) {
      let current_tail = self.ledgers.get(endorser_handle.as_slice());
      return Ok(current_tail.unwrap().to_vec());
    }
    Err(EndorserError::StateCreationError)
  }

  pub fn append_ledger(
    &mut self,
    endorser_handle: Vec<u8>,
    updated_hash: Vec<u8>,
  ) -> Result<(Vec<u8>, Vec<u8>, Signature), EndorserError> {
    if self.ledgers.contains_key(&*endorser_handle.clone()) {
      let (handle, previous_hash) = self
        .ledgers
        .get_key_value(&*endorser_handle.clone())
        .unwrap();
      let previous_hash_bytes = previous_hash.clone();
      let current_hash = updated_hash.clone();
      let concat_hashes = concat_bytes(previous_hash, current_hash.as_slice());
      let tail_hash = hash(concat_hashes.as_slice());
      let signature = self.keypair.sign(tail_hash.as_slice());
      self
        .ledgers
        .insert(handle.to_vec(), tail_hash.clone().to_vec());
      return Ok((previous_hash_bytes, tail_hash.clone().to_vec(), signature));
    }
    Err(EndorserError::StateCreationError)
  }

  pub fn get_endorser_key_info_from_endorser_state(&self) -> EndorserIdentity {
    self.id
  }
}

impl Store {
  pub fn new() -> Self {
    Store {
      state: EndorserState::new(),
    }
  }

  // Returns the creation of a new EndorserState and the Signed Key corresponding to it
  // Explicitly refreshes the keys in the keystate for testing purposes.
  pub fn create_new_endorser_state(&mut self) -> Result<(String, EndorserIdentity), EndorserError> {
    let mut endorser_state = EndorserState::new();
    let identity = endorser_state.id.clone();
    let data = concat_bytes(identity.pubkey.as_bytes(), &identity.sign.to_bytes());
    println!("PK    : {:?}", identity.pubkey.to_bytes());
    println!("Sign  : {:?}", identity.sign.to_bytes());
    println!("Concat: {:?}", data);

    let endorser_handle_index = Sha3_256::digest(&*data).to_vec();
    println!("Hash  : {:?}", endorser_handle_index);

    let response = EndorserIdentity {
      pubkey: identity.pubkey,
      sign: identity.sign,
    };
    let endorser_handle = hex::encode(endorser_handle_index);
    self.state = endorser_state;
    Ok((endorser_handle.to_string(), response))
  }

  pub fn create_new_ledger_in_endorser_state(
    &mut self,
    coordinator_handle: Vec<u8>,
  ) -> Result<(Vec<u8>, Signature, EndorserIdentity), EndorserError> {
    println!("Received Coordinator Handle: {:?}", coordinator_handle);
    let (handle, ledger_response, public_key) = self
      .state
      .create_ledger(coordinator_handle)
      .expect("Unable to create a Ledger in EndorserState");
    Ok((handle, ledger_response, self.state.id))
  }

  pub fn get_all_available_handles(&self) -> Vec<Vec<u8>> {
    self.state.get_handles_in_endorser_state().unwrap()
  }

  pub fn get_endorser_key_information(&self) -> Result<EndorserIdentity, EndorserError> {
    let id = self.state.get_endorser_key_info_from_endorser_state();
    Ok(id)
  }

  pub fn append_and_update_endorser_state_tail(
    &mut self,
    endorsor_handle: Vec<u8>,
    updated_data: Vec<u8>,
  ) -> Result<(Vec<u8>, Vec<u8>, Signature), EndorserError> {
    let handle = &endorsor_handle.clone();
    println!("Handle Queried: {:?}", handle);
    let updated_hash = hash(updated_data.as_slice());
    let (previous_state, tail, signature) = self
      .state
      .append_ledger(handle.clone(), updated_hash.to_vec())
      .unwrap();
    Ok((previous_state, tail, signature))
  }

  pub fn get_latest_state_for_handle(
    &self,
    handle: Vec<u8>,
    nonce: Vec<u8>,
  ) -> Result<(Vec<u8>, Vec<u8>, Signature), EndorserError> {
    let tail_hash = self.state.get_tail(handle);
    if tail_hash.is_ok() {
      let tail_hash_bytes = tail_hash.unwrap().to_vec();
      let concat_result = concat_bytes(tail_hash_bytes.as_slice(), nonce.as_slice());
      let content_to_sign = hash(concat_result.as_slice());
      let endorser_signature = self.state.keypair.sign(content_to_sign.as_slice());
      return Ok((nonce, tail_hash_bytes, endorser_signature));
    }
    Err(EndorserError::TailDoesNotMatch)
  }
}

mod tests {
  use super::*;
  use ed25519_dalek::ed25519::signature::Signature;
  use rand::Rng;

  #[test]
  pub fn check_endorser_identity_methods() {
    let endorser_state = EndorserState::new();
    let endorser_identity = endorser_state.id;
    let public_key = endorser_identity.get_public_key();
    let signature = endorser_identity.get_signature();
    let signature_instance = Signature::from_bytes(&signature.as_slice()).unwrap();
    let sig_verification = endorser_state
      .keypair
      .verify(public_key.as_slice(), &signature_instance);
    if sig_verification.is_ok() {
      let d = sig_verification.unwrap();
      println!("{:?}", d);
      assert_eq!(2, 2);
    }

    // Taken from reference test vectors in ed25519-dalek
    let incorrect_signature_hex: &[u8] = b"98a70222f0b8121aa9d30f813d683f809e462b469c7ff87639499bb94e6dae4131f85042463c2a355a2003d062adf5aaa10b8c61e636062aaad11c2a26083406";
    let incorrect_sig_bytes: Vec<u8> = hex::decode(incorrect_signature_hex).unwrap();
    let incorrect_signature = Signature::from_bytes(&incorrect_sig_bytes[..]).unwrap();

    let wrong_signature_verification = endorser_state
      .keypair
      .verify(public_key.as_slice(), &incorrect_signature);
    if wrong_signature_verification.is_ok() {
      panic!("This should have failed but it did not.")
    } else {
      println!("Succeeded.")
    }
  }

  #[test]
  pub fn check_endorser_state_creation() {
    let endorser_state = EndorserState::new();
    let key_information = endorser_state.keypair;
    let public_key = key_information.public.to_bytes();
    let secret_key = key_information.secret.to_bytes();
    assert_eq!(public_key.len(), 32usize);
    assert_eq!(secret_key.len(), 32usize);

    let endorser_identity = endorser_state.id;
    let endorser_identity_public_key = endorser_identity.pubkey.to_bytes();
    let endorser_identity_signature = endorser_identity.sign.to_bytes();
    // The signature in the SGX will be a sign of PK and measurement parameters of the SGX
    assert_eq!(public_key, endorser_identity_public_key);
    assert_eq!(endorser_identity_signature.len(), 64usize);
    assert_eq!(
      endorser_identity_signature,
      key_information
        .sign(endorser_identity_public_key.to_vec().as_slice())
        .to_bytes()
    );
  }

  #[test]
  pub fn check_endorser_new_ledger_and_get_tail() {
    let mut endorser_state = EndorserState::new();
    // The coordinator sends the hashed contents of the block to the
    let genesis_block_hash = rand::thread_rng().gen::<[u8; 32]>();
    let create_ledger_endorser_response = endorser_state.create_ledger(genesis_block_hash.to_vec());
    if create_ledger_endorser_response.is_ok() {
      let (handle, signature, public_key) = create_ledger_endorser_response.unwrap();
      assert_eq!(handle, genesis_block_hash);
      assert_eq!(public_key, endorser_state.keypair.public);

      let signature_expected = endorser_state.keypair.sign(&genesis_block_hash);
      assert_eq!(signature, signature_expected);

      // Fetch the value currently in the tail.
      let tail_result = endorser_state.get_tail(genesis_block_hash.to_vec());
      if tail_result.is_ok() {
        let tail_hash = tail_result.unwrap();
        assert_eq!(tail_hash, hash(&signature_expected.to_bytes()).to_vec());
      } else {
        panic!("Failed to retrieve correct tail hash on genesis ledger state creation");
      }
    } else {
      panic!("Failed to create ledger using genesis hash at the ledger");
    }
  }

  #[test]
  pub fn check_endorser_append_ledger_tail() {
    let mut endorser_state = EndorserState::new();

    let genesis_block_hash = rand::thread_rng().gen::<[u8; 32]>();
    let create_ledger_endorser_response = endorser_state.create_ledger(genesis_block_hash.to_vec());

    let (handle, signature, public_key) = create_ledger_endorser_response.unwrap();

    // Fetch the value currently in the tail.
    let tail_result = endorser_state
      .get_tail(genesis_block_hash.to_vec())
      .unwrap();

    let updated_content_from_coordinator = "this is a new message from coordinator".as_bytes();
    let updated_hash = hash(updated_content_from_coordinator).to_vec();
    let (previous_state, tail, signature) = endorser_state
      .append_ledger(handle.clone(), updated_hash.clone())
      .unwrap();

    let expected_tail_contents = concat_bytes(&tail_result, &updated_hash);
    let expected_tail = hash(&expected_tail_contents).to_vec();

    assert_eq!(previous_state.as_slice(), tail_result.as_slice());
    assert_eq!(tail.as_slice(), expected_tail.as_slice());

    let tail_signature_verification = endorser_state.keypair.verify(tail.as_slice(), &signature);

    if tail_signature_verification.is_ok() {
      println!("Verification Passed. Checking Updated Tail");
      let new_tail = endorser_state.get_tail(handle).unwrap();
      assert_eq!(new_tail.as_slice(), tail.as_slice());
      assert_eq!(new_tail.as_slice(), expected_tail.as_slice());
    } else {
      panic!("Signature verification failed when it should not have failed");
    }
  }
}
