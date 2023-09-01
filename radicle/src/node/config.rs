use std::collections::HashSet;
use std::{fmt, str, net};

use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;
use thiserror::Error;

use crate::node;
use crate::node::tracking::{Policy, Scope};
use crate::node::{Address, Alias, NodeId};

/// Target number of peers to maintain connections to.
pub const TARGET_OUTBOUND_PEERS: usize = 8;

/// Peer-to-peer network.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Network {
    #[default]
    Main,
    Test,
}

impl Network {
    /// Bootstrap nodes for this network.
    pub fn bootstrap(&self) -> Vec<(Alias, NodeId, Address)> {
        use std::str::FromStr;

        match self {
            Self::Main => [
                (
                    "seed.radicle.garden",
                    "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776",
                ),
                (
                    "seed.radicle.xyz",
                    "z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz@seed.radicle.xyz:8776",
                ),
            ]
            .into_iter()
            // SAFETY: These are valid addresses.
            .map(|(a, s)| {
                let alias = Alias::new(a);
                let PeerAddr { id: nid, addr } = PeerAddr::from_str(s).unwrap();
                (alias, nid, addr)
            })
            .collect(),

            Self::Test => vec![],
        }
    }
}

/// Configuration parameters defining attributes of minima and maxima.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// Number of routing table entries before we start pruning.
    pub routing_max_size: usize,
    /// How long to keep a routing table entry before being pruned.
    #[serde(with = "crate::serde_ext::localtime::duration")]
    pub routing_max_age: LocalDuration,
    /// How long to keep a gossip message entry before pruning it.
    #[serde(with = "crate::serde_ext::localtime::duration")]
    pub gossip_max_age: LocalDuration,
    /// Maximum number of concurrent fetches per per connection.
    pub fetch_concurrency: usize,
    /// Rate limitter settings.
    #[serde(default)]
    pub rate: RateLimits,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            routing_max_size: 1000,
            routing_max_age: LocalDuration::from_mins(7 * 24 * 60), // One week
            gossip_max_age: LocalDuration::from_mins(2 * 7 * 24 * 60), // Two weeks
            fetch_concurrency: 1,
            rate: RateLimits::default(),
        }
    }
}

/// Rate limts for a single connection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    pub fill_rate: f64,
    pub capacity: usize,
}

/// Rate limits for inbound and outbound connections.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimits {
    pub inbound: RateLimit,
    pub outbound: RateLimit,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self {
            inbound: RateLimit {
                fill_rate: 0.2,
                capacity: 32,
            },
            outbound: RateLimit {
                fill_rate: 1.0,
                capacity: 64,
            },
        }
    }
}

/// Peer configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PeerConfig {
    /// Static peer set. Connect to the configured peers and maintain the connections.
    Static,
    /// Dynamic peer set.
    Dynamic { target: usize },
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self::Dynamic {
            target: TARGET_OUTBOUND_PEERS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct ConnectAddress {
    pub node_id: NodeId,
    pub addr_opt: Option<Address>,
}

impl From<PeerAddr<NodeId, Address>> for ConnectAddress {
    fn from(value: PeerAddr<NodeId, Address>) -> Self {
        let PeerAddr { id, addr } = value;
        ConnectAddress {
            node_id: id,
            addr_opt: Some(addr),
        }
    }
}

impl TryFrom<String> for ConnectAddress {
    type Error = ConnectAddressParseError;

    fn try_from(s: String) -> Result<Self, ConnectAddressParseError> {
        str::FromStr::from_str(&s)
    }
}

impl Into<String> for ConnectAddress {
    fn into(self) -> String {
        self.to_string()
    }
}

impl From<ConnectAddress> for (NodeId, Option<Address>) {
    fn from(value: ConnectAddress) -> Self {
        (value.node_id, value.addr_opt)
    }
}

impl From<(NodeId, Address)> for ConnectAddress {
    fn from((id, addr): (NodeId, Address)) -> Self {
        ConnectAddress {
            node_id: id,
            addr_opt: Some(addr),
        }
    }
}

impl From<(NodeId, Option<Address>)> for ConnectAddress {
    fn from((node_id, addr_opt): (NodeId, Option<Address>)) -> Self {
        ConnectAddress { node_id, addr_opt }
    }
}

#[derive(Error, Debug)]
pub enum ConnectAddressParseError {
    #[error("invalid node id")]
    InvalidNodeId(crypto::PublicKeyError),
    #[error("invalid address")]
    InvalidAddress(cyphernet::addr::AddrParseError),
}

impl str::FromStr for ConnectAddress {
    type Err = ConnectAddressParseError;

    fn from_str(s: &str) -> Result<Self, ConnectAddressParseError> {
        let ret = match s.split_once('@') {
            None => {
                let node_id =
                    NodeId::from_str(s).map_err(ConnectAddressParseError::InvalidNodeId)?;
                ConnectAddress {
                    node_id,
                    addr_opt: None,
                }
            }
            Some((node_id, addr)) => {
                let node_id =
                    NodeId::from_str(node_id).map_err(ConnectAddressParseError::InvalidNodeId)?;
                let addr =
                    Address::from_str(addr).map_err(ConnectAddressParseError::InvalidAddress)?;
                ConnectAddress {
                    node_id,
                    addr_opt: Some(addr),
                }
            }
        };
        Ok(ret)
    }
}

impl fmt::Display for ConnectAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.addr_opt {
            None => write!(f, "{}", self.node_id),
            Some(addr) => write!(f, "{}@{}", self.node_id, addr),
        }
    }
}

/// Service configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Node alias.
    pub alias: Alias,
    /// Address to listen on.
    #[serde(default)]
    pub listen: Vec<net::SocketAddr>,
    /// Peer configuration.
    #[serde(default)]
    pub peers: PeerConfig,
    /// Peers to connect to on startup.
    /// Connections to these peers will be maintained.
    #[serde(default)]
    pub connect: HashSet<ConnectAddress>,
    /// Specify the node's public addresses
    #[serde(default)]
    pub external_addresses: Vec<Address>,
    /// Peer-to-peer network.
    #[serde(default)]
    pub network: Network,
    /// Whether or not our node should relay inventories.
    #[serde(default = "crate::serde_ext::bool::yes")]
    pub relay: bool,
    /// Configured service limits.
    #[serde(default)]
    pub limits: Limits,
    /// Default tracking policy.
    #[serde(default)]
    pub policy: Policy,
    /// Default tracking scope.
    #[serde(default)]
    pub scope: Scope,
}

impl Config {
    pub fn test(alias: Alias) -> Self {
        Self {
            network: Network::Test,
            ..Self::new(alias)
        }
    }

    pub fn new(alias: Alias) -> Self {
        Self {
            alias,
            peers: PeerConfig::default(),
            listen: vec![],
            connect: HashSet::default(),
            external_addresses: vec![],
            network: Network::default(),
            relay: true,
            limits: Limits::default(),
            policy: Policy::default(),
            scope: Scope::default(),
        }
    }
}

impl Config {
    pub fn peer(&self, id: &NodeId) -> Option<Option<&Address>> {
        self.connect.iter().find_map(|connect_addr| {
            (connect_addr.node_id == *id).then_some(connect_addr.addr_opt.as_ref())
        })
    }

    pub fn is_persistent(&self, id: &NodeId) -> bool {
        self.peer(id).is_some()
    }

    pub fn features(&self) -> node::Features {
        node::Features::SEED
    }
}
