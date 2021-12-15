use crate::{
    config::Configure,
    message::{self, Propose},
    types::Command,
};
use log::{debug, trace};
use madsim::net::NetLocalHandle;
use std::{io::Result, marker::PhantomData, net::SocketAddr};

pub struct Client<C>
where
    C: Command,
{
    net: NetLocalHandle,
    conf: Configure,
    addr: SocketAddr,
    phantom: PhantomData<C>,
}

impl<C> Client<C>
where
    C: Command,
{
    pub async fn new(conf: Configure, id: usize) -> Self {
        let addr = conf
            .peers
            .get(id)
            .or_else(|| panic!("id {} is not in the configure scope", id))
            .unwrap()
            .clone();

        Self {
            net: NetLocalHandle::current(),
            conf,
            addr,
            phantom: PhantomData,
        }
    }

    pub async fn propose(&mut self, cmds: Vec<C>) -> Result<()> {
        trace!("start propose");
        let propose = Propose {
            cmds: cmds
                .iter()
                .map(|c| bincode::serialize(c).unwrap())
                .collect(),
        };
        self.net.call(self.addr, propose).await?;
        Ok(())
    }
}
