mod errors;

use tonic::{
  transport::{Channel, Endpoint},
  Request,
};

#[allow(clippy::derive_partial_eq_without_eq)]
pub mod coordinator_proto {
  tonic::include_proto!("coordinator_proto");
}

use crate::errors::EndpointError;
use coordinator_proto::{
  call_client::CallClient, AppendReq, AppendResp, NewLedgerReq, NewLedgerResp, ReadLatestReq,
  ReadLatestResp, ReadViewByIndexReq, ReadViewByIndexResp, ReadViewTailReq, ReadViewTailResp, GetTimeoutMapReq, GetTimeoutMapResp, PingAllReq, PingAllResp, AddEndorsersReq, AddEndorsersResp
};
use ledger::{
  errors::VerificationError,
  signature::{PrivateKey, PrivateKeyTrait, PublicKey, PublicKeyTrait, Signature, SignatureTrait},
  Block, CustomSerde, NimbleDigest, NimbleHashTrait, VerifierState,
};
use rand::random;
use std::{
  collections::HashMap, convert::TryFrom, sync::{Arc, RwLock}
};

#[allow(dead_code)]
enum MessageType {
  NewCounterReq,
  NewCounterResp,
  IncrementCounterReq,
  IncrementCounterResp,
  ReadCounterReq,
  ReadCounterResp,
}

const DEFAULT_NUM_GRPC_CHANNELS: usize = 1;

#[derive(Debug, Clone)]
pub struct Connection {
  clients: Vec<CallClient<Channel>>,
  num_grpc_channels: usize,
}

impl Connection {
  /// Creates a new connection to the coordinator.
  pub async fn new(
    coordinator_endpoint_address: String,
    num_grpc_channels_opt: Option<usize>,
  ) -> Result<Self, EndpointError> {
    let num_grpc_channels = match num_grpc_channels_opt {
      Some(n) => n,
      None => DEFAULT_NUM_GRPC_CHANNELS,
    };
    let mut clients = Vec::new();
    for _idx in 0..num_grpc_channels {
      let connection_attempt = Endpoint::from_shared(coordinator_endpoint_address.clone());
      let connection = match connection_attempt {
        Ok(connection) => connection,
        Err(_err) => return Err(EndpointError::CoordinatorHostNameNotFound),
      };
      let channel = connection.connect_lazy();
      let client = CallClient::new(channel);
      clients.push(client);
    }
    Ok(Self {
      clients,
      num_grpc_channels,
    })
  }

  /// Creates a new ledger with the given handle and block.
  pub async fn new_ledger(&self, handle: &[u8], block: &[u8]) -> Result<Vec<u8>, EndpointError> {
    let req = Request::new(NewLedgerReq {
      handle: handle.to_vec(),
      block: block.to_vec(),
    });
    let NewLedgerResp { receipts } = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .new_ledger(req)
      .await
      .map_err(|e| {
        eprintln!("Failed to create a new ledger {:?}", e);
        EndpointError::FailedToCreateNewCounter
      })?
      .into_inner();
    Ok(receipts)
  }

  /// Appends a block to the ledger with the given handle and expected height.
  pub async fn append(
    &self,
    handle: &[u8],
    block: &[u8],
    expected_height: u64,
  ) -> Result<(Vec<u8>, Vec<u8>), EndpointError> {
    let req = Request::new(AppendReq {
      handle: handle.to_vec(),
      block: block.to_vec(),
      expected_height,
    });
    let AppendResp {
      hash_nonces,
      receipts,
    } = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .append(req)
      .await
      .map_err(|e| {
        eprintln!("Failed to append to a ledger {:?}", e);
        EndpointError::FailedToIncrementCounter
      })?
      .into_inner();
    Ok((hash_nonces, receipts))
  }

  /// Reads the latest block from the ledger with the given handle and nonce.
  pub async fn read_latest(
    &self,
    handle: &[u8],
    nonce: &[u8],
  ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), EndpointError> {
    let ReadLatestResp {
      block,
      nonces,
      receipts,
    } = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .read_latest(ReadLatestReq {
        handle: handle.to_vec(),
        nonce: nonce.to_vec(),
      })
      .await
      .map_err(|e| {
        eprintln!("Failed to read a ledger {:?}", e);
        EndpointError::FailedToReadCounter
      })?
      .into_inner();
    Ok((block, nonces, receipts))
  }

  /// Reads a block from the view ledger by index.
  pub async fn read_view_by_index(
    &self,
    index: usize,
  ) -> Result<(Vec<u8>, Vec<u8>), EndpointError> {
    let ReadViewByIndexResp { block, receipts } = self.clients
      [random::<usize>() % self.num_grpc_channels]
      .clone()
      .read_view_by_index(ReadViewByIndexReq {
        index: index as u64,
      })
      .await
      .map_err(|_e| EndpointError::FailedToReadViewLedger)?
      .into_inner();
    Ok((block, receipts))
  }

  /// Reads the tail of the view ledger.
  pub async fn read_view_tail(&self) -> Result<(Vec<u8>, Vec<u8>, usize, Vec<u8>), EndpointError> {
    let ReadViewTailResp {
      block,
      receipts,
      height,
      attestations,
    } = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .read_view_tail(ReadViewTailReq {})
      .await
      .map_err(|_e| EndpointError::FailedToReadViewLedger)?
      .into_inner();
    Ok((block, receipts, height as usize, attestations))
  }

  /// Gets the timeout map from the coordinator.
  pub async fn get_timeout_map(
    &self,
  ) -> Result<HashMap<String, u64>, EndpointError> {
    let GetTimeoutMapResp {
      timeout_map,
    } = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .get_timeout_map(GetTimeoutMapReq {})
      .await
      .map_err(|_e| EndpointError::FailedToGetTimeoutMap)?
      .into_inner();
    Ok(timeout_map)
  }

  /// Pings all endorsers.
  pub async fn ping_all_endorsers(
    &self,
  ) -> Result<(), EndpointError> {
    let PingAllResp {} = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .ping_all_endorsers(PingAllReq {})
      .await
      .map_err(|_e| EndpointError::FailedToPingAllEndorsers)?
      .into_inner();
    Ok(())
  }

  /// Adds endorsers with the given URI.
  pub async fn add_endorsers(
    &self,
    uri: String,
  ) -> Result<(), EndpointError> {
    let AddEndorsersResp {} = self.clients[random::<usize>() % self.num_grpc_channels]
      .clone()
      .add_endorsers(AddEndorsersReq {
        endorsers: uri,
      })
      .await
      .map_err(|_e| EndpointError::FailedToAddEndorsers)?
      .into_inner();
    Ok(())
  }
}

pub struct EndpointState {
  conn: Connection,
  id: NimbleDigest,
  sk: PrivateKey,
  pk: PublicKey,
  vs: Arc<RwLock<VerifierState>>,
}

#[derive(Debug)]
pub enum PublicKeyFormat {
  UNCOMPRESSED = 0,
  COMPRESSED = 1,
  DER = 2,
}

#[derive(Debug)]
pub enum SignatureFormat {
  RAW = 0,
  DER = 1,
}

impl EndpointState {
  /// Creates a new endpoint state.
  pub async fn new(
    hostname: String,
    pem_opt: Option<String>,
    num_grpc_channels_opt: Option<usize>,
  ) -> Result<Self, EndpointError> {
    // make a connection to the coordinator
    let conn = {
      let res = Connection::new(hostname, num_grpc_channels_opt).await;

      match res {
        Ok(conn) => conn,
        Err(e) => {
          panic!("Endpoint Error: {:?}", e);
        },
      }
    };

    // initialize id and vs
    let (id, vs) = {
      let mut vs = VerifierState::default();

      let (block, _r) = conn.read_view_by_index(1usize).await.unwrap();

      // the hash of the genesis block of the view ledger uniquely identifies a particular instance of NimbleLedger
      let id = Block::from_bytes(&block).unwrap().hash();
      vs.set_group_identity(id);

      let (block, receipts, height, attestations) = conn.read_view_tail().await.unwrap();
      let res = vs.apply_view_change(&block, &receipts, Some(&attestations));
      assert!(res.is_ok());

      for index in (1..height).rev() {
        let (block, receipts) = conn.read_view_by_index(index).await.unwrap();
        let res = vs.apply_view_change(&block, &receipts, None);
        assert!(res.is_ok());
      }

      (id, vs)
    };

    // produce a private key pair to sign responses
    let sk = if let Some(pem) = pem_opt {
      let res = PrivateKey::from_pem(pem.as_bytes());
      if let Err(error) = res {
        panic!("Endpoint Error: {:?}", error);
      }
      res.unwrap()
    } else {
      PrivateKey::new()
    };

    let pk = sk.get_public_key().unwrap();

    Ok(EndpointState {
      conn,
      id,
      sk,
      pk,
      vs: Arc::new(RwLock::new(vs)),
    })
  }

  /// Gets the identity of the endpoint.
  pub fn get_identity(
    &self,
    pkformat: PublicKeyFormat,
  ) -> Result<(Vec<u8>, Vec<u8>), EndpointError> {
    let public_key = self.sk.get_public_key().unwrap();
    Ok((
      self.id.to_bytes(),
      match pkformat {
        PublicKeyFormat::COMPRESSED => public_key.to_bytes(),
        PublicKeyFormat::DER => public_key.to_der(),
        _ => public_key.to_uncompressed(),
      },
    ))
  }

  /// Updates the view of the endpoint.
  async fn update_view(&self) -> Result<(), EndpointError> {
    let start_height = {
      if let Ok(vs_rd) = self.vs.read() {
        vs_rd.get_view_ledger_height() + 1
      } else {
        return Err(EndpointError::FailedToAcquireReadLock);
      }
    };

    let (block, receipts, height, attestations) = self.conn.read_view_tail().await.unwrap();
    if let Ok(mut vs_wr) = self.vs.write() {
      let res = vs_wr.apply_view_change(&block, &receipts, Some(&attestations));
      if res.is_err() {
        return Err(EndpointError::FailedToApplyViewChange);
      }
    } else {
      return Err(EndpointError::FailedToAcquireWriteLock);
    }

    for index in (start_height..height).rev() {
      let (block, receipts) = self.conn.read_view_by_index(index).await.unwrap();
      if let Ok(mut vs_wr) = self.vs.write() {
        let res = vs_wr.apply_view_change(&block, &receipts, None);
        if res.is_err() {
          return Err(EndpointError::FailedToApplyViewChange);
        }
      } else {
        return Err(EndpointError::FailedToAcquireWriteLock);
      }
    }

    Ok(())
  }

  /// Creates a new counter with the given handle, tag, and signature format.
  pub async fn new_counter(
    &self,
    handle: &[u8],
    tag: &[u8],
    sigformat: SignatureFormat,
  ) -> Result<Vec<u8>, EndpointError> {
    // construct a block that unequivocally identifies the client's intent to create a new counter
    let block = {
      let msg = {
        let s = format!(
          "{}.{}.{}.{}.{}",
          base64_url::encode(&(MessageType::NewCounterReq as u64).to_le_bytes()),
          base64_url::encode(&self.id.to_bytes()),
          base64_url::encode(handle),
          base64_url::encode(&0_u64.to_le_bytes()),
          base64_url::encode(tag),
        );
        NimbleDigest::digest(s.as_bytes())
      };

      let sig = self.sk.sign(&msg.to_bytes()).unwrap();

      // concatenate tag and signature
      [tag.to_vec(), sig.to_bytes()].concat()
    };

    // issue a request to the coordinator and receive a response
    let receipts = {
      let res = self.conn.new_ledger(handle, &block).await;
      if res.is_err() {
        return Err(EndpointError::FailedToCreateNewCounter);
      }
      res.unwrap()
    };

    // verify the response received from the coordinator;
    let res = {
      if let Ok(vs_rd) = self.vs.read() {
        vs_rd.verify_new_ledger(handle, &block, &receipts)
      } else {
        return Err(EndpointError::FailedToAcquireReadLock);
      }
    };

    if res.is_err() {
      if res.unwrap_err() != VerificationError::ViewNotFound {
        return Err(EndpointError::FailedToVerifyNewCounter);
      } else {
        let res = self.update_view().await;
        if res.is_err() {
          return Err(EndpointError::FailedToVerifyNewCounter);
        }
        let res = {
          if let Ok(vs_rd) = self.vs.read() {
            vs_rd.verify_new_ledger(handle, &block, &receipts)
          } else {
            return Err(EndpointError::FailedToAcquireReadLock);
          }
        };
        if res.is_err() {
          eprintln!("failed to create a new counter {:?}", res);
          return Err(EndpointError::FailedToVerifyNewCounter);
        }
      }
    }

    // sign a message that unequivocally identifies the counter and tag
    let msg = {
      let s = format!(
        "{}.{}.{}.{}.{}",
        base64_url::encode(&(MessageType::NewCounterResp as u64).to_le_bytes()),
        base64_url::encode(&self.id.to_bytes()),
        base64_url::encode(handle),
        base64_url::encode(&0_u64.to_le_bytes()),
        base64_url::encode(tag),
      );
      NimbleDigest::digest(s.as_bytes())
    };
    let sig = self.sk.sign(&msg.to_bytes()).unwrap();
    let signature = match sigformat {
      SignatureFormat::DER => sig.to_der(),
      _ => sig.to_bytes(),
    };

    Ok(signature)
  }

  /// Increments the counter with the given handle, tag, expected counter, and signature format.
  pub async fn increment_counter(
    &self,
    handle: &[u8],
    tag: &[u8],
    expected_counter: u64,
    sigformat: SignatureFormat,
  ) -> Result<Vec<u8>, EndpointError> {
    // convert u64 to usize, returning error
    let expected_height = {
      let res = usize::try_from(expected_counter);
      if res.is_err() {
        return Err(EndpointError::FailedToConvertCounter);
      }
      res.unwrap()
    };

    // construct a block that unequivocally identifies the client's intent to update the counter and tag
    let block = {
      let msg = {
        let s = format!(
          "{}.{}.{}.{}.{}",
          base64_url::encode(&(MessageType::IncrementCounterReq as u64).to_le_bytes()),
          base64_url::encode(&self.id.to_bytes()),
          base64_url::encode(handle),
          base64_url::encode(&expected_counter.to_le_bytes()),
          base64_url::encode(tag),
        );
        NimbleDigest::digest(s.as_bytes())
      };

      let sig = self.sk.sign(&msg.to_bytes()).unwrap();

      [tag.to_vec(), sig.to_bytes()].concat()
    };

    // issue a request to the coordinator and receive a response
    let (hash_nonces, receipts) = {
      let res = self.conn.append(handle, &block, expected_counter).await;

      if res.is_err() {
        return Err(EndpointError::FailedToIncrementCounter);
      }
      res.unwrap()
    };

    // verify the response received from the coordinator; TODO: handle the case where vs does not have the returned view hash
    let res = {
      if let Ok(vs_rd) = self.vs.read() {
        vs_rd.verify_append(handle, &block, &hash_nonces, expected_height, &receipts)
      } else {
        return Err(EndpointError::FailedToAcquireReadLock);
      }
    };
    if res.is_err() {
      if res.unwrap_err() != VerificationError::ViewNotFound {
        return Err(EndpointError::FailedToVerifyIncrementedCounter);
      } else {
        let res = self.update_view().await;
        if res.is_err() {
          return Err(EndpointError::FailedToVerifyIncrementedCounter);
        }
        let res = {
          if let Ok(vs_rd) = self.vs.read() {
            vs_rd.verify_append(handle, &block, &hash_nonces, expected_height, &receipts)
          } else {
            return Err(EndpointError::FailedToAcquireReadLock);
          }
        };
        if res.is_err() {
          eprintln!("failed to increment a counter {:?}", res);
          return Err(EndpointError::FailedToVerifyIncrementedCounter);
        }
      }
    }

    // sign a message that unequivocally identifies the counter and tag
    let msg = {
      let s = format!(
        "{}.{}.{}.{}.{}",
        base64_url::encode(&(MessageType::IncrementCounterResp as u64).to_le_bytes()),
        base64_url::encode(&self.id.to_bytes()),
        base64_url::encode(handle),
        base64_url::encode(&expected_height.to_le_bytes()),
        base64_url::encode(tag),
      );
      NimbleDigest::digest(s.as_bytes())
    };
    let sig = self.sk.sign(&msg.to_bytes()).unwrap();
    let signature = match sigformat {
      SignatureFormat::DER => sig.to_der(),
      _ => sig.to_bytes(),
    };

    Ok(signature)
  }

  /// Reads the counter with the given handle, nonce, and signature format.
  pub async fn read_counter(
    &self,
    handle: &[u8],
    nonce: &[u8],
    sigformat: SignatureFormat,
  ) -> Result<(Vec<u8>, u64, Vec<u8>), EndpointError> {
    // issue a request to the coordinator and receive a response
    let (block, nonces, receipts) = {
      let res = self.conn.read_latest(handle, nonce).await;

      if res.is_err() {
        return Err(EndpointError::FailedToReadCounter);
      }
      res.unwrap()
    };

    // verify the response received from the coordinator
    let res = {
      if let Ok(vs_rd) = self.vs.read() {
        vs_rd.verify_read_latest(handle, &block, &nonces, nonce, &receipts)
      } else {
        return Err(EndpointError::FailedToAcquireReadLock);
      }
    };
    let counter = {
      if res.is_err() {
        if res.unwrap_err() != VerificationError::ViewNotFound {
          return Err(EndpointError::FaieldToVerifyReadCounter);
        } else {
          let res = self.update_view().await;
          if res.is_err() {
            return Err(EndpointError::FaieldToVerifyReadCounter);
          }
          let res = {
            if let Ok(vs_rd) = self.vs.read() {
              vs_rd.verify_read_latest(handle, &block, &nonces, nonce, &receipts)
            } else {
              return Err(EndpointError::FailedToAcquireReadLock);
            }
          };
          if res.is_err() {
            return Err(EndpointError::FaieldToVerifyReadCounter);
          } else {
            res.unwrap()
          }
        }
      } else {
        res.unwrap()
      }
    };

    // verify the integrity of the coordinator's response by checking the signature
    if block.len() < Signature::num_bytes() {
      return Err(EndpointError::FaieldToVerifyReadCounter);
    }
    let (tag, sig) = {
      let (t, s) = block.split_at(block.len() - Signature::num_bytes());
      assert_eq!(t.len(), block.len() - Signature::num_bytes());
      assert_eq!(s.len(), Signature::num_bytes());
      (t, Signature::from_bytes(s).unwrap())
    };

    let msg = {
      let s = format!(
        "{}.{}.{}.{}.{}",
        base64_url::encode(&if counter == 0 {
          (MessageType::NewCounterReq as u64).to_le_bytes()
        } else {
          (MessageType::IncrementCounterReq as u64).to_le_bytes()
        }),
        base64_url::encode(&self.id.to_bytes()),
        base64_url::encode(handle),
        base64_url::encode(&(counter as u64).to_le_bytes()),
        base64_url::encode(&tag),
      );
      NimbleDigest::digest(s.as_bytes())
    };

    if sig.verify(&self.pk, &msg.to_bytes()).is_err() {
      return Err(EndpointError::FaieldToVerifyReadCounter);
    }

    // sign a message to the client that unequivocally identifies the counter and tag
    let msg = {
      let s = format!(
        "{}.{}.{}.{}.{}.{}",
        base64_url::encode(&(MessageType::ReadCounterResp as u64).to_le_bytes()),
        base64_url::encode(&self.id.to_bytes()),
        base64_url::encode(handle),
        base64_url::encode(&(counter as u64).to_le_bytes()),
        base64_url::encode(&tag),
        base64_url::encode(nonce),
      );
      NimbleDigest::digest(s.as_bytes())
    };
    let sig = self.sk.sign(&msg.to_bytes()).unwrap();
    let signature = match sigformat {
      SignatureFormat::DER => sig.to_der(),
      _ => sig.to_bytes(),
    };

    // respond to the light client
    Ok((tag.to_vec(), counter as u64, signature))
  }

  /// Gets the timeout map from the coordinator.
  pub async fn get_timeout_map(
    &self
  ) -> Result<HashMap<String, u64>, EndpointError> {
    

    let timeout_map = {
      let res = self.conn.get_timeout_map().await;

      if res.is_err() {
        return Err(EndpointError::FailedToGetTimeoutMap);
      }
      res.unwrap()
    };

    // respond to the light client
    Ok(timeout_map)
  }

  /// Pings all endorsers.
  pub async fn ping_all_endorsers(
    &self,
  ) -> Result<(), EndpointError> {
    

    let _block = {
      let res = self.conn.ping_all_endorsers().await;

      if res.is_err() {
        return Err(EndpointError::FailedToPingAllEndorsers);
      }
      res.unwrap()
    };

    // respond to the light client
    Ok(())
  }

  /// Adds endorsers with the given URI.
  pub async fn add_endorsers(
    &self,
    uri: String,
  ) -> Result<(), EndpointError> {
    

    let _block = {
      let res = self.conn.add_endorsers(uri).await;

      if res.is_err() {
        return Err(EndpointError::FailedToAddEndorsers);
      }
      res.unwrap()
    };

    // respond to the light client
    Ok(())
  }
}
