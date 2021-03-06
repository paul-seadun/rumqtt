use mqtt4bytes::{Packet, PubAck};
use std::collections::{HashMap, VecDeque};
use std::mem;

type Pkid = u16;
type Offset = u64;
type Topic = String;

/// Watermarks for a given topic
#[derive(Debug)]
pub struct Watermarks {
    pending_acks_reply: bool,
    /// Packet id to offset map per topic. When replication requirements
    /// are met, packet ids will be moved to acks
    pkid_offset_map: HashMap<Topic, (VecDeque<Pkid>, VecDeque<Offset>)>,
    /// Committed packet ids for acks
    acks: Vec<(Pkid, Packet)>,
    /// Offset till which replication has happened (per mesh node)
    cluster_offsets: Vec<Offset>,
}

impl Watermarks {
    pub fn new() -> Watermarks {
        Watermarks {
            pending_acks_reply: false,
            pkid_offset_map: HashMap::new(),
            acks: Vec::new(),
            cluster_offsets: vec![0, 0, 0],
        }
    }

    pub fn update_cluster_offsets(&mut self, id: usize, offset: u64) {
        if let Some(position) = self.cluster_offsets.get_mut(id) {
            *position = offset
        } else {
            panic!(
                "We only support a maximum of 3 nodes at the moment. Received id = {}",
                id
            );
        }

        // debug!("Updating cluster offsets. Topic = {}, Offsets: {:?}", self.topic, self.cluster_offsets);
    }

    pub fn set_pending_acks_reply(&mut self, status: bool) {
        self.pending_acks_reply = status
    }

    pub fn pending_acks_reply(&self) -> bool {
        self.pending_acks_reply
    }

    /// Commit acks with enough replication
    pub fn commit(&mut self, topic: &str) {
        let connection = self.pkid_offset_map.get_mut(topic).unwrap();
        let highest_replicated_offset = *self.cluster_offsets.iter().max().unwrap();

        // cut offsets which are less than highest replicated offset
        // e.g. For connection = 30, router id = 0, pkid_offset_map and replica offsets looks like this
        // pkid offset map = [5, 4, 3, 2, 1] : [15, 14, 10, 9, 8]
        // replica offsets = [0, 12, 8] implies replica 1 has replicated till 12 and replica 2 till 8
        // the above example should return pkids [5, 4]

        // get index of offset less than replicated offset and split there
        // TODO: Fix this with a normal loop as there is pkids loop anyway
        if let Some(index) = connection
            .1
            .iter()
            .position(|x| *x <= highest_replicated_offset)
        {
            connection.1.truncate(index);
            let pkids = connection.0.split_off(index);
            for pkid in pkids {
                let puback = PubAck::new(pkid);
                self.acks.push((pkid, Packet::PubAck(puback)));
            }
        }
    }

    pub fn push_ack(&mut self, pkid: u16, packet: Packet) {
        self.acks.push((pkid, packet))
    }

    /// Returns committed acks by take
    pub fn acks(&mut self) -> Vec<(Pkid, Packet)> {
        let acks = mem::replace(&mut self.acks, Vec::new());
        acks
    }

    pub fn update_pkid_offset_map(&mut self, topic: &str, pkid: u16, offset: u64) {
        // connection ids which are greater than supported count should be rejected during
        // connection itself. Crashing here is a bug
        let map = match self.pkid_offset_map.get_mut(topic) {
            Some(map) => map,
            None => {
                self.pkid_offset_map
                    .insert(topic.to_owned(), (VecDeque::new(), VecDeque::new()));
                self.pkid_offset_map.get_mut(topic).unwrap()
            }
        };

        map.0.push_front(pkid);
        map.1.push_front(offset);
    }
}
