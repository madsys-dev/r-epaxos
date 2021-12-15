use crate::types::{Ballot, InstanceId, LeaderId, LocalInstanceId, Seq};
use madsim::Request;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Request)]
#[rtype("PreAcceptReply")]
pub(crate) struct PreAccept {
    pub(crate) leader_id: LeaderId,
    pub(crate) instance_id: InstanceId,
    pub(crate) seq: Seq,
    pub(crate) ballot: Ballot,
    pub(crate) cmds: Vec<Vec<u8>>,
    pub(crate) deps: Vec<Option<LocalInstanceId>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PreAcceptReply {
    pub(crate) instance_id: InstanceId,
    pub(crate) deps: Option<PreAcceptReplyDeps>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PreAcceptReplyDeps {
    pub(crate) seq: Seq,
    pub(crate) ballot: Ballot,
    pub(crate) ok: bool,
    pub(crate) deps: Vec<Option<LocalInstanceId>>,
    pub(crate) committed_deps: Vec<Option<LocalInstanceId>>,
}

#[derive(Debug, Serialize, Deserialize, Request)]
#[rtype("AcceptReply")]
pub(crate) struct Accept {
    pub(crate) leader_id: LeaderId,
    pub(crate) instance_id: InstanceId,
    pub(crate) ballot: Ballot,
    pub(crate) seq: Seq,
    pub(crate) cmd_cnt: usize,
    pub(crate) deps: Vec<Option<LocalInstanceId>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AcceptReply {
    instance_id: InstanceId,
    ok: bool,
    ballot: Ballot,
}

#[derive(Debug, Serialize, Deserialize, Request)]
#[rtype("()")]
pub(crate) struct Commit {
    pub(crate) leader_id: LeaderId,
    pub(crate) instance_id: InstanceId,
    pub(crate) seq: Seq,
    pub(crate) cmds: Vec<Vec<u8>>,
    pub(crate) deps: Vec<Option<LocalInstanceId>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CommitShort {
    leader_id: LeaderId,
    instance_id: InstanceId,
    seq: Seq,
    cmd_cnt: usize,
    deps: Vec<InstanceId>,
}

#[derive(Debug, Serialize, Deserialize, Request)]
#[rtype("()")]
pub(crate) struct Propose {
    pub(crate) cmds: Vec<Vec<u8>>,
}
