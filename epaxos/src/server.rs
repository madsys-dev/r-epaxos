use crate::{
    config::Configure,
    message::{Accept, Commit, PreAccept, PreAcceptReply, PreAcceptReplyDeps, Propose},
    types::{
        Ballot, Command, Instance, InstanceId, InstanceStatus, LeaderBook, LeaderId,
        LocalInstanceId, Replica, ReplicaId,
    },
};
use itertools::Itertools;
use madsim::net::NetLocalHandle;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

#[derive(Clone)]
pub struct Server<C>
where
    C: Command,
{
    inner: Arc<Mutex<InnerServer<C>>>,
}

struct InnerServer<C>
where
    C: Command,
{
    conf: Configure,
    replica: Replica<C>,
}

impl<C> InnerServer<C>
where
    C: Command,
{
    fn new(conf: Configure) -> Self {
        let peer_cnt = conf.peers().len();
        let id = conf.index;
        Self {
            conf,
            replica: Replica::new(id, peer_cnt),
        }
    }

    fn conf(&self) -> &Configure {
        &self.conf
    }
}

#[madsim::service]
impl<C> Server<C>
where
    C: Command,
{
    pub async fn new(conf: Configure) -> Self {
        let server = Self {
            inner: Arc::new(Mutex::new(InnerServer::new(conf))),
        };
        server.add_rpc_handler();
        server
    }

    #[rpc]
    async fn handle_commit(&self, cm: Commit) {
        trace!("handle commit");
        let mut inner = self.inner.lock().await;
        let r = &mut inner.replica;

        if cm.instance_id.inner >= r.cur_instance(&cm.instance_id.replica) {
            r.set_cur_instance(&InstanceId {
                replica: cm.instance_id.replica,
                inner: (*cm.instance_id.inner + 1).into(),
            });
        }

        let ins = r.instance_space[*cm.instance_id.replica].get(*cm.instance_id.inner);

        match ins {
            Some(ins) => {
                let mut ins = ins.write().await;
                ins.seq = cm.seq;
                ins.deps = cm.deps;
                ins.status = InstanceStatus::Committed;
            }
            None => {
                drop(ins);
                let cmds = cm
                    .cmds
                    .iter()
                    .map(|bytes| bincode::deserialize(bytes).unwrap())
                    .collect_vec();
                let new_instance = Arc::new(RwLock::new(Instance {
                    id: cm.instance_id.inner,
                    seq: cm.seq,
                    ballot: Ballot::new(),
                    cmds: cmds.clone(),
                    deps: cm.deps,
                    status: InstanceStatus::Committed,
                    lb: LeaderBook::new(),
                }));
                r.instance_space[*cm.instance_id.replica]
                    .insert(*cm.instance_id.inner, new_instance.clone());
                r.update_conflict(&cmds, new_instance).await;
            }
        }

        // TODO: sync to disk
    }

    async fn handle_preaccept_reply(&self, par: PreAcceptReply) {
        trace!("handle preaccept reply");

        let mut inner = self.inner.lock().await;
        let InnerServer { conf, replica: r } = &mut *inner;

        let mut ins = r.instance_space[*par.instance_id.replica]
            .get(*par.instance_id.inner)
            .expect("This instance should already in the space")
            .write()
            .await;

        if !matches!(ins.status, InstanceStatus::PreAccepted) {
            // We've translated to the later states
            return;
        }

        if let Some(par) = &par.deps {
            if ins.ballot != par.ballot {
                // Other advanced (larger ballot) leader is handling
                return;
            }

            if !par.ok {
                ins.lb.nack += 1;
                if par.ballot > ins.lb.max_ballot {
                    ins.lb.max_ballot = par.ballot;
                }
                return;
            }
        } else {
            if !ins.ballot.is_init() {
                // only the first leader can send ok
                return;
            }
        }

        ins.lb.preaccept_ok += 1;

        if let Some(par) = &par.deps {
            let equal = r.merge_seq_deps(&mut ins, &par.seq, &par.deps);
            if ins.lb.preaccept_ok > 1 {
                ins.lb.all_equal = ins.lb.all_equal && equal;
            }
        }

        if ins.lb.preaccept_ok >= r.peer_cnt / 2 && ins.lb.all_equal && ins.ballot.is_init() {
            ins.status = InstanceStatus::Committed;
            // TODO: sync to disk

            // Broadcast Commit
            for (id, &peer) in conf.peers().iter().enumerate() {
                if id == *r.id {
                    continue;
                }
                let msg = Commit {
                    leader_id: r.id.into(),
                    instance_id: par.instance_id,
                    seq: ins.seq,
                    cmds: ins
                        .cmds
                        .iter()
                        .map(|c| bincode::serialize(c).unwrap())
                        .collect(),
                    deps: ins.deps.to_vec(),
                };
                let this = self.clone();
                madsim::task::spawn(async move {
                    let net = NetLocalHandle::current();
                    net.call(peer, msg).await.unwrap();
                })
                .detach();
            }
        } else if ins.lb.preaccept_ok >= r.peer_cnt / 2 {
            ins.status = InstanceStatus::Accepted;
            // Broadcast Accept
            for (id, &peer) in conf.peers().iter().enumerate() {
                if id == *r.id {
                    continue;
                }
                let msg = Accept {
                    leader_id: r.id.into(),
                    instance_id: par.instance_id,
                    ballot: ins.ballot,
                    seq: ins.seq,
                    cmd_cnt: ins.cmds.len(),
                    deps: ins.deps.to_vec(),
                };
                let this = self.clone();
                madsim::task::spawn(async move {
                    let net = NetLocalHandle::current();
                    let reply = net.call(peer, msg).await.unwrap();
                    todo!("handle: {:?}", reply);
                })
                .detach();
            }
        }
    }

    #[rpc]
    async fn handle_preaccept(&self, pa: PreAccept) -> PreAcceptReply {
        trace!("handle preaccept {:?}", pa);
        let mut inner = self.inner.lock().await;
        let r = &mut inner.replica;
        let cmds = pa
            .cmds
            .iter()
            .map(|bytes| bincode::deserialize(bytes).unwrap())
            .collect();

        if let Some(inst) = r.instance_space[*pa.instance_id.replica].get(*pa.instance_id.inner) {
            let inst_read = inst.read().await;

            // We've got accpet or commit before, don't reply
            if matches!(
                inst_read.status,
                InstanceStatus::Committed | InstanceStatus::Accepted
            ) {
                // Later message may not contain commands, we should fill it here
                if inst_read.cmds.is_empty() {
                    drop(inst_read);
                    let mut inst_write = inst.write().await;
                    inst_write.cmds = cmds;
                }
                todo!("return something?");
                // return;
            }

            // smaller Ballot number
            if pa.ballot < inst_read.ballot {
                return PreAcceptReply {
                    instance_id: pa.instance_id,
                    deps: Some(PreAcceptReplyDeps {
                        seq: inst_read.seq,
                        ballot: inst_read.ballot,
                        ok: false,
                        deps: inst_read.deps.to_vec(),
                        committed_deps: r.commited_upto.to_vec(),
                    }),
                };
            }
        }

        if pa.instance_id.inner > r.cur_instance(&pa.instance_id.replica) {
            r.set_cur_instance(&pa.instance_id);
        }

        // FIXME: We'd better not copy dep vec
        let (seq, deps, changed) = r.update_seq_deps(pa.seq, pa.deps.to_vec(), &cmds).await;

        let status = if changed {
            InstanceStatus::PreAccepted
        } else {
            InstanceStatus::PreAcceptedEq
        };

        let uncommited_deps = r
            .commited_upto
            .iter()
            .enumerate()
            .map(|cu| {
                if let Some(cu_id) = cu.1 {
                    if let Some(d) = deps[cu.0] {
                        if cu_id < &d {
                            return true;
                        }
                    }
                }
                return false;
            })
            .filter(|a| *a)
            .count()
            > 0;

        let new_instance = Arc::new(RwLock::new(Instance {
            id: pa.instance_id.inner,
            seq,
            ballot: pa.ballot,
            // FIXME: should not copy
            cmds: cmds.clone(),
            // FIXME: Should not copy if we send reply ok later
            deps: deps.to_vec(),
            status,
            lb: LeaderBook::new(),
        }));
        r.instance_space[*pa.instance_id.replica]
            .insert(*pa.instance_id.inner, new_instance.clone());

        r.update_conflict(&cmds, new_instance).await;

        // TODO: sync to disk

        // Send Reply
        if changed
            || uncommited_deps
            || *pa.instance_id.replica != *pa.leader_id
            || !pa.ballot.is_init()
        {
            PreAcceptReply {
                instance_id: pa.instance_id,
                deps: Some(PreAcceptReplyDeps {
                    seq,
                    ballot: pa.ballot,
                    ok: true,
                    deps,
                    // Should not copy
                    committed_deps: r.commited_upto.to_vec(),
                }),
            }
        } else {
            PreAcceptReply {
                instance_id: pa.instance_id,
                deps: None,
            }
        }
    }

    #[rpc]
    async fn handle_propose(&self, p: Propose) {
        trace!("handle propose");
        let mut inner = self.inner.lock().await;
        let InnerServer { conf, replica: r } = &mut *inner;

        let inst_no = *r.local_cur_instance();
        r.inc_local_cur_instance();
        let cmds = p
            .cmds
            .iter()
            .map(|bytes| bincode::deserialize(bytes).unwrap())
            .collect();
        let (seq, deps) = r.get_seq_deps(&cmds).await;

        let new_inst = Arc::new(RwLock::new(Instance {
            id: inst_no,
            seq,
            ballot: 0.into(),
            cmds,
            deps,
            status: InstanceStatus::PreAccepted,
            lb: LeaderBook::new(),
        }));
        r.update_conflict(&new_inst.read().await.cmds, new_inst.clone())
            .await;

        let r_id = *r.id;
        r.instance_space[r_id].insert(*inst_no, new_inst.clone());

        if seq > r.max_seq {
            r.max_seq = (*seq + 1).into();
        }

        // TODO: Flush the content to disk

        // Broadcast PreAccept
        for (id, &peer) in conf.peers().iter().enumerate() {
            if id == *r.id {
                continue;
            }
            let msg = PreAccept {
                leader_id: r.id.into(),
                instance_id: InstanceId {
                    replica: r.id,
                    inner: inst_no,
                },
                seq,
                ballot: 0.into(),
                // FIXME: should not copy/clone vec
                cmds: new_inst
                    .read()
                    .await
                    .cmds
                    .iter()
                    .map(|c| bincode::serialize(c).unwrap())
                    .collect(),
                deps: new_inst.read().await.deps.to_vec(),
            };
            let this = self.clone();
            madsim::task::spawn(async move {
                let net = NetLocalHandle::current();
                let reply = net.call(peer, msg).await.unwrap();
                this.handle_preaccept_reply(reply).await;
            })
            .detach();
        }
    }
}
