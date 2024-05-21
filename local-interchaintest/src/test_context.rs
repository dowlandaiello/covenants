use std::{collections::HashMap, path::PathBuf};

use localic_std::{modules::cosmwasm::CosmWasm, relayer::{Channel, Relayer}, transactions::ChainRequestBuilder};

use crate::{
    chain_tests::{find_pairwise_ccv_channel_ids, find_pairwise_transfer_channel_ids},
    ibc_helpers, types::ChainsVec, utils::API_URL,
};


pub struct TestContext {
    pub chains: HashMap<String, LocalChain>,
    // maps (src_chain_id, dest_chain_id) to transfer channel id
    pub transfer_channel_ids: HashMap<(String, String), String>,
    // maps (src_chain_id, dest_chain_id) to ccv channel id
    pub ccv_channel_ids: HashMap<(String, String), String>,
    // maps (src_chain_id, dest_chain_id) to connection id
    pub connection_ids: HashMap<(String, String), String>,
    // maps (src_chain_id, dest_chain_id) to src chain native
    // denom -> ibc denom on dest chain
    pub ibc_denoms: HashMap<(String, String), String>,
}

impl From<ChainsVec> for TestContext {
    fn from(chains: ChainsVec) -> Self {
        let mut chains_map = HashMap::new();
        for chain in chains.chains {
            let rb = ChainRequestBuilder::new(
                API_URL.to_string(),
                chain.chain_id.clone(),
                chain.debugging,
            )
            .unwrap();

            let relayer: Relayer = Relayer::new(&rb);
            let channels = relayer.get_channels(&rb.chain_id).unwrap();
            for (i, channel) in channels.iter().enumerate() {
                println!("{} channel #{}: {:?}", rb.chain_id, i, channel);
            }

            let (src_addr, denom) = match rb.chain_id.as_str() {
                "localneutron-1" => ("neutron1hj5fveer5cjtn4wd6wstzugjfdxzl0xpznmsky", "untrn"),
                "localcosmos-1" => ("cosmos1hj5fveer5cjtn4wd6wstzugjfdxzl0xpxvjjvr", "uatom"),
                "localstride-3" => ("stride1u20df3trc2c2zdhm8qvh2hdjx9ewh00sv6eyy8", "ustrd"),
                _ => ("err", "err"),
            };
            let local_chain =
                LocalChain::new(rb, src_addr.to_string(), denom.to_string(), channels);
            chains_map.insert(chain.name.clone(), local_chain);
        }

        let mut ntrn_channels = chains_map.get("neutron").unwrap().channels.clone();
        let mut gaia_channels = chains_map.get("gaia").unwrap().channels.clone();
        let mut stride_channels = chains_map.get("stride").unwrap().channels.clone();

        let mut connection_ids = HashMap::new();

        let (ntrn_to_gaia_consumer_channel, gaia_to_ntrn_provider_channel) =
            find_pairwise_ccv_channel_ids(&gaia_channels, &ntrn_channels).unwrap();

        ntrn_channels.remove(ntrn_to_gaia_consumer_channel.index);
        gaia_channels.remove(gaia_to_ntrn_provider_channel.index);
        connection_ids.insert(
            ("neutron".to_string(), "gaia".to_string()),
            ntrn_to_gaia_consumer_channel.connection_id,
        );
        connection_ids.insert(
            ("gaia".to_string(), "neutron".to_string()),
            gaia_to_ntrn_provider_channel.connection_id,
        );

        let (ntrn_to_gaia_transfer_channel, gaia_to_ntrn_transfer_channel) =
            find_pairwise_transfer_channel_ids(&ntrn_channels, &gaia_channels).unwrap();
        ntrn_channels.remove(ntrn_to_gaia_transfer_channel.index);
        gaia_channels.remove(gaia_to_ntrn_transfer_channel.index);

        let (ntrn_to_stride_transfer_channel, stride_to_ntrn_transfer_channel) =
            find_pairwise_transfer_channel_ids(&ntrn_channels, &stride_channels).unwrap();
        ntrn_channels.remove(ntrn_to_stride_transfer_channel.index);
        stride_channels.remove(stride_to_ntrn_transfer_channel.index);
        connection_ids.insert(
            ("neutron".to_string(), "stride".to_string()),
            ntrn_to_stride_transfer_channel.connection_id,
        );
        connection_ids.insert(
            ("stride".to_string(), "neutron".to_string()),
            stride_to_ntrn_transfer_channel.connection_id,
        );

        let (gaia_to_stride_transfer_channel, stride_to_gaia_transfer_channel) =
            find_pairwise_transfer_channel_ids(&gaia_channels, &stride_channels).unwrap();
        gaia_channels.remove(gaia_to_stride_transfer_channel.index);
        stride_channels.remove(stride_to_gaia_transfer_channel.index);
        connection_ids.insert(
            ("gaia".to_string(), "stride".to_string()),
            gaia_to_stride_transfer_channel.connection_id,
        );
        connection_ids.insert(
            ("stride".to_string(), "gaia".to_string()),
            stride_to_gaia_transfer_channel.connection_id,
        );

        let mut transfer_channel_ids = HashMap::new();
        transfer_channel_ids.insert(
            ("neutron".to_string(), "stride".to_string()),
            ntrn_to_stride_transfer_channel.channel_id.to_string(),
        );
        transfer_channel_ids.insert(
            ("stride".to_string(), "neutron".to_string()),
            stride_to_ntrn_transfer_channel.channel_id.to_string(),
        );
        transfer_channel_ids.insert(
            ("gaia".to_string(), "stride".to_string()),
            gaia_to_stride_transfer_channel.channel_id.to_string(),
        );
        transfer_channel_ids.insert(
            ("stride".to_string(), "gaia".to_string()),
            stride_to_gaia_transfer_channel.channel_id.to_string(),
        );
        transfer_channel_ids.insert(
            ("neutron".to_string(), "gaia".to_string()),
            ntrn_to_gaia_transfer_channel.channel_id.to_string(),
        );
        transfer_channel_ids.insert(
            ("gaia".to_string(), "neutron".to_string()),
            gaia_to_ntrn_transfer_channel.channel_id.to_string(),
        );

        let mut ccv_channel_ids = HashMap::new();
        ccv_channel_ids.insert(
            ("gaia".to_string(), "neutron".to_string()),
            gaia_to_ntrn_provider_channel.channel_id,
        );
        ccv_channel_ids.insert(
            ("neutron".to_string(), "gaia".to_string()),
            ntrn_to_gaia_consumer_channel.channel_id,
        );

        let mut ibc_denoms = HashMap::new();
        ibc_denoms.insert(
            ("neutron".to_string(), "stride".to_string()),
            ibc_helpers::get_ibc_denom(
                "untrn",
                &ntrn_to_stride_transfer_channel.channel_id,
            ),
        );
        ibc_denoms.insert(
            ("stride".to_string(), "neutron".to_string()),
            ibc_helpers::get_ibc_denom(
                "ustrd",
                &stride_to_ntrn_transfer_channel.channel_id,
            ),
        );
        ibc_denoms.insert(
            ("gaia".to_string(), "stride".to_string()),
            ibc_helpers::get_ibc_denom(
                "uatom",
                &gaia_to_stride_transfer_channel.channel_id,
            ),
        );
        ibc_denoms.insert(
            ("stride".to_string(), "gaia".to_string()),
            ibc_helpers::get_ibc_denom(
                "ustrd",
                &stride_to_gaia_transfer_channel.channel_id,
            ),
        );
        ibc_denoms.insert(
            ("neutron".to_string(), "gaia".to_string()),
            ibc_helpers::get_ibc_denom(
                "untrn",
                &ntrn_to_gaia_transfer_channel.channel_id,
            ),
        );
        ibc_denoms.insert(
            ("gaia".to_string(), "neutron".to_string()),
            ibc_helpers::get_ibc_denom(
                "uatom",
                &gaia_to_ntrn_transfer_channel.channel_id,
            ),
        );

        Self {
            chains: chains_map,
            transfer_channel_ids,
            ccv_channel_ids,
            connection_ids,
            ibc_denoms,
        }
    }
}

pub struct LocalChain {
    /// ChainRequestBuilder
    pub rb: ChainRequestBuilder,
    /// contract codes stored on this chain (filename -> code_id)
    pub contract_codes: HashMap<String, u64>,
    /// outgoing channel ids
    pub channels: Vec<Channel>,
    /// outgoing connection ids available (dest_chain_id -> connection_id)
    pub connection_ids: HashMap<String, String>,
    pub admin_addr: String,
    pub native_denom: String,
}

impl LocalChain {
    pub fn new(
        rb: ChainRequestBuilder,
        admin_addr: String,
        native_denom: String,
        channels: Vec<Channel>,
    ) -> Self {
        Self {
            rb,
            contract_codes: Default::default(),
            channels,
            connection_ids: Default::default(),
            admin_addr,
            native_denom,
        }
    }

    pub fn get_cw(&mut self) -> CosmWasm {
        CosmWasm::new(&self.rb)
    }

    pub fn save_code(&mut self, abs_path: PathBuf, code: u64) {
        let id = abs_path.file_stem().unwrap().to_str().unwrap();
        self.contract_codes.insert(id.to_string(), code);
    }
}


impl TestContext {
    pub fn get_transfer_channels(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::TransferChannel)
    }

    pub fn get_connections(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::Connection)
    }

    pub fn get_ccv_channels(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::CCVChannel)
    }

    pub fn get_ibc_denoms(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::IBCDenom)
    }

    pub fn get_admin_addr(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::AdminAddr)
    }

    pub fn get_native_denom(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::NativeDenom)
    }

    pub fn get_request_builder(&self) -> TestContextQuery {
        TestContextQuery::new(self, QueryType::RequestBuilder)
    }
}

pub enum QueryType {
    TransferChannel,
    Connection,
    CCVChannel,
    IBCDenom,
    AdminAddr,
    NativeDenom,
    RequestBuilder,
}

pub struct TestContextQuery<'a> {
    context: &'a TestContext,
    query_type: QueryType,
    src_chain: Option<String>,
    dest_chain: Option<String>,
    contract_name: Option<String>,
}

impl<'a> TestContextQuery<'a> {
    pub fn new(context: &'a TestContext, query_type: QueryType) -> Self {
        Self {
            context,
            query_type,
            src_chain: None,
            dest_chain: None,
            contract_name: None,
        }
    }

    pub fn src(mut self, src_chain: &str) -> Self {
        self.src_chain = Some(src_chain.to_string());
        self
    }

    pub fn dest(mut self, dest_chain: &str) -> Self {
        self.dest_chain = Some(dest_chain.to_string());
        self
    }

    pub fn contract(mut self, contract_name: &str) -> Self {
        self.contract_name = Some(contract_name.to_string());
        self
    }

    pub fn get(self) -> String {
        let query_response = match self.query_type {
            QueryType::TransferChannel => self.get_transfer_channel(),
            QueryType::Connection => self.get_connection_id(),
            QueryType::CCVChannel => self.get_ccv_channel(),
            QueryType::IBCDenom => self.get_ibc_denom(),
            QueryType::AdminAddr => self.get_admin_addr(),
            QueryType::NativeDenom => self.get_native_denom(),
            _ => None,
        };
        query_response.unwrap()
    }

    pub fn get_all(self) -> Vec<String> {
        match self.query_type {
            QueryType::TransferChannel => self.get_all_transfer_channels(),
            QueryType::Connection => self.get_all_connections(),
            _ => vec![],
        }
    }

    pub fn get_request_builder(mut self, chain: &str) -> &'a ChainRequestBuilder {
        self.src_chain = Some(chain.to_string());
        let rb = match self.query_type {
            QueryType::RequestBuilder => self.get_rb(),
            _ => None,
        };
        rb.unwrap()
    }

    fn get_transfer_channel(self) -> Option<String> {
        if let (Some(ref src), Some(ref dest)) = (self.src_chain, self.dest_chain) {
            self.context.transfer_channel_ids.get(&(src.clone(), dest.clone())).cloned()
        } else {
            None
        }
    }

    fn get_all_transfer_channels(self) -> Vec<String> {
        if let Some(ref src) = self.src_chain {
            self.context
                .transfer_channel_ids
                .iter()
                .filter(|((s, _), _)| s == src)
                .map(|(_, v)| v.clone())
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    }

    fn get_connection_id(self) -> Option<String> {
        if let (Some(ref src), Some(ref dest)) = (self.src_chain, self.dest_chain) {
            self.context.connection_ids.get(&(src.clone(), dest.clone())).cloned()
        } else {
            None
        }
    }

    fn get_all_connections(self) -> Vec<String> {
        if let Some(ref src) = self.src_chain {
            self.context
                .connection_ids
                .iter()
                .filter(|((s, _), _)| s == src)
                .map(|(_, v)| v.clone())
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    }

    fn get_ccv_channel(self) -> Option<String> {
        if let (Some(ref src), Some(ref dest)) = (self.src_chain, self.dest_chain) {
            self.context.ccv_channel_ids.get(&(src.clone(), dest.clone())).cloned()
        } else {
            None
        }
    }

    fn get_ibc_denom(self) -> Option<String> {
        if let (Some(ref src), Some(ref dest)) = (self.src_chain, self.dest_chain) {
            self.context.ibc_denoms.get(&(src.clone(), dest.clone())).cloned()
        } else {
            None
        }
    }

    fn get_admin_addr(self) -> Option<String> {
        if let Some(ref src) = self.src_chain {
            self.context.chains.get(src).map(|chain| chain.admin_addr.clone())
        } else {
            None
        }
    }

    fn get_native_denom(self) -> Option<String> {
        if let Some(ref src) = self.src_chain {
            self.context.chains.get(src).map(|chain| chain.native_denom.clone())
        } else {
            None
        }
    }

    fn get_rb(self) -> Option<&'a ChainRequestBuilder> {
        if let Some(ref src) = self.src_chain {
            self.context.chains.get(src).map(|chain| &chain.rb)
        } else {
            None
        }
    }
}
