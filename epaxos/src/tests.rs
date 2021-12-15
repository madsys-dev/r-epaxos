use crate::error::ExecuteError;
use crate::server::Server;
use crate::types::Command;
use crate::{client::Client, config::Configure};
use log::debug;
use madsim::Handle;
use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum TestCommand {
    Read { key: String },
    Write { key: String, value: String },
}

#[async_trait::async_trait]
impl Command for TestCommand {
    type K = String;

    fn key(&self) -> &Self::K {
        match self {
            Self::Read { key } => key,
            Self::Write { key, .. } => key,
        }
    }

    async fn execute<F>(&self, f: F) -> Result<(), ExecuteError>
    where
        F: Fn(&Self) -> Result<(), ExecuteError> + Send,
    {
        f(self)
    }
}

#[madsim::test]
async fn propose() {
    const N: usize = 3;
    let handle = Handle::current();

    let peers = (0..N)
        .map(|i| SocketAddr::from(([192, 168, 1, i as u8 + 1], 1)))
        .collect::<Vec<_>>();
    let mut servers = Vec::with_capacity(N);
    for id in 0..N {
        let cfg = Configure::new(peers.clone(), id);
        let server = handle
            .create_host(peers[id])
            .unwrap()
            .spawn(Server::<TestCommand>::new(cfg))
            .await;
        servers.push(server);
    }

    let mut client = Client::<TestCommand>::new(Configure::new(peers, 0), 0).await;
    client
        .propose(vec![TestCommand::Write {
            key: "k1".to_owned(),
            value: "v1".to_owned(),
        }])
        .await
        .unwrap();
}
