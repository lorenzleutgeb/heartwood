use std::collections::HashSet;
use std::net;
use std::ops::Deref;

use cyphernet::addr::PeerAddr;
use localtime::LocalDuration;

use crate::node;
use crate::node::policy::{Policy, Scope};
use crate::node::{Address, Alias, NodeId};

/// Target number of peers to maintain connections to.
pub const TARGET_OUTBOUND_PEERS: usize = 8;

/// Configured public seeds.
pub mod seeds {
    use std::str::FromStr;

    use super::{ConnectAddress, PeerAddr};
    use once_cell::sync::Lazy;

    /// The radicle public community seed node.
    pub static RADICLE_COMMUNITY_NODE: Lazy<ConnectAddress> = Lazy::new(|| {
        // SAFETY: `ConnectAddress` is known at compile time.
        #[allow(clippy::unwrap_used)]
        PeerAddr::from_str(
            "z6MkrLMMsiPWUcNPHcRajuMi9mDfYckSoJyPwwnknocNYPm7@seed.radicle.garden:8776",
        )
        .unwrap()
        .into()
    });

    /// The radicle team node.
    pub static RADICLE_TEAM_NODE: Lazy<ConnectAddress> = Lazy::new(|| {
        // SAFETY: `ConnectAddress` is known at compile time.
        #[allow(clippy::unwrap_used)]
        PeerAddr::from_str("z6MksmpU5b1dS7oaqF2bHXhQi1DWy2hB7Mh9CuN7y1DN6QSz@seed.radicle.xyz:8776")
            .unwrap()
            .into()
    });
}

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
    pub fn bootstrap(&self) -> Vec<(Alias, ConnectAddress)> {
        match self {
            Self::Main => [
                ("seed.radicle.garden", seeds::RADICLE_COMMUNITY_NODE.clone()),
                ("seed.radicle.xyz", seeds::RADICLE_TEAM_NODE.clone()),
            ]
            .into_iter()
            .map(|(a, s)| (Alias::new(a), s))
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

/// Full address used to connect to a remote node.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Hash)]
#[serde(transparent)]
pub struct ConnectAddress(#[serde(with = "crate::serde_ext::string")] PeerAddr<NodeId, Address>);

impl From<PeerAddr<NodeId, Address>> for ConnectAddress {
    fn from(value: PeerAddr<NodeId, Address>) -> Self {
        Self(value)
    }
}

impl From<ConnectAddress> for (NodeId, Address) {
    fn from(value: ConnectAddress) -> Self {
        (value.0.id, value.0.addr)
    }
}

impl From<(NodeId, Address)> for ConnectAddress {
    fn from((id, addr): (NodeId, Address)) -> Self {
        Self(PeerAddr { id, addr })
    }
}

impl From<ConnectAddress> for Address {
    fn from(value: ConnectAddress) -> Self {
        value.0.addr
    }
}

impl Deref for ConnectAddress {
    type Target = PeerAddr<NodeId, Address>;

    fn deref(&self) -> &Self::Target {
        &self.0
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
    /// Default seeding policy.
    #[serde(default)]
    pub policy: Policy,
    /// Default seeding scope.
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

    /// Configuration for a test seed node.
    ///
    /// It sets the `RateLimit::capacity` to `usize::MAX` ensuring
    /// that there are no rate limits for test nodes, since they all
    /// operate on the same IP address. This prevents any announcement
    /// messages from being dropped.
    pub fn seed(alias: Alias) -> Self {
        Self {
            network: Network::Test,
            limits: Limits {
                rate: RateLimits {
                    inbound: RateLimit {
                        fill_rate: 1.0,
                        capacity: usize::MAX,
                    },
                    outbound: RateLimit {
                        fill_rate: 1.0,
                        capacity: usize::MAX,
                    },
                },
                ..Limits::default()
            },
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
    pub fn peer(&self, id: &NodeId) -> Option<&Address> {
        self.connect
            .iter()
            .find(|ca| &ca.id == id)
            .map(|ca| &ca.addr)
    }

    pub fn is_persistent(&self, id: &NodeId) -> bool {
        self.peer(id).is_some()
    }

    pub fn features(&self) -> node::Features {
        node::Features::SEED
    }
}
