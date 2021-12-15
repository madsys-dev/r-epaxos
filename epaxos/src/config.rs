use std::net::SocketAddr;
use std::ops::Index;
use yaml_rust::YamlLoader;

#[derive(Clone)]
pub struct Configure {
    pub(crate) peers: Vec<SocketAddr>,
    pub(crate) index: usize,
}

impl Configure {
    // This should only be used in test
    pub(crate) fn new(peers: Vec<SocketAddr>, index: usize) -> Self {
        Self { peers, index }
    }

    pub fn peers(&self) -> &[SocketAddr] {
        &self.peers
    }
}

impl Index<usize> for Configure {
    type Output = SocketAddr;

    fn index(&self, index: usize) -> &Self::Output {
        &self.peers[index]
    }
}

pub trait ConfigureSrc {
    fn get_configure(&self) -> Configure;
}

/// Read Configure from regular file
pub struct YamlConfigureSrc {
    yaml: String,
}

impl YamlConfigureSrc {
    pub fn new(yaml: &str) -> Self {
        Self {
            yaml: yaml.to_owned(),
        }
    }
}

impl ConfigureSrc for YamlConfigureSrc {
    fn get_configure(&self) -> Configure {
        let yaml = YamlLoader::load_from_str(&self.yaml).unwrap();
        if yaml.len() != 1 {
            panic!("We should only pass in a yaml file");
        }

        // have checked length
        let yaml = yaml.get(0).unwrap();

        // TODO: put string to const
        let _peer_cnt = yaml["peer_cnt"].as_i64().unwrap() as usize;

        let mut peers = yaml["peer"]
            .as_vec()
            .unwrap()
            .iter()
            .map(|y| y.as_str().unwrap().parse().unwrap())
            .collect();

        let index = yaml["index"].as_i64().unwrap() as usize;

        Configure { peers, index }
    }
}
