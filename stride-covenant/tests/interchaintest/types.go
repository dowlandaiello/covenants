package ibc_test

type DepositorInstantiateMsg struct {
	StAtomReceiver                  WeightedReceiver `json:"st_atom_receiver"`
	AtomReceiver                    WeightedReceiver `json:"atom_receiver"`
	ClockAddress                    string           `json:"clock_address,string"`
	GaiaNeutronIBCTransferChannelId string           `json:"gaia_neutron_ibc_transfer_channel_id"`
	GaiaStrideIBCTransferChannelId  string           `json:"gaia_stride_ibc_transfer_channel_id"`
	NeutronGaiaConnectionId         string           `json:"neutron_gaia_connection_id"`
}

type LPerInstantiateMsg struct {
	LpPosition    LpInfo `json:"lp_position"`
	ClockAddress  string `json:"clock_address,string"`
	HolderAddress string `json:"holder_address,string"`
}

type LpInfo struct {
	Addr string `json:"addr,string"`
}

type WeightedReceiver struct {
	Amount  int64  `json:"amount"`
	Address string `json:"address,string"`
}

// A query against the Neutron example contract. Note the usage of
// `omitempty` on fields. This means that if that field has no value,
// it will not have a key in the serialized representaiton of the
// struct, thus mimicing the serialization of Rust enums.
type IcaExampleContractQuery struct {
	InterchainAccountAddress InterchainAccountAddressQuery `json:"interchain_account_address,omitempty"`
}

type InterchainAccountAddressQuery struct {
	InterchainAccountId string `json:"interchain_account_id"`
	ConnectionId        string `json:"connection_id"`
}

type QueryResponse struct {
	Data InterchainAccountAddressQueryResponse `json:"data"`
}

type ICAQueryResponse struct {
	Data DepositorInterchainAccountAddressQueryResponse `json:"data"`
}

type InterchainAccountAddressQueryResponse struct {
	InterchainAccountAddress string `json:"interchain_account_address"`
}

type DepositorICAAddressQuery struct {
	DepositorInterchainAccountAddress DepositorInterchainAccountAddressQuery `json:"depositor_interchain_account_address"`
}

type DepositorContractQuery struct {
	ClockAddress ClockAddressQuery `json:"clock_address"`
}

type LPContractQuery struct {
	ClockAddress ClockAddressQuery `json:"clock_address"`
}

type LPPositionQuery struct {
	LpPosition LpPositionQuery `json:"lp_position"`
}

type StAtomWeightedReceiverQuery struct {
	StAtomReceiver StAtomReceiverQuery `json:"st_atom_receiver"`
}

type AtomWeightedReceiverQuery struct {
	AtomReceiver AtomReceiverQuery `json:"atom_receiver"`
}

type ClockAddressQuery struct{}
type StAtomReceiverQuery struct{}
type AtomReceiverQuery struct{}
type DepositorInterchainAccountAddressQuery struct{}
type LpPositionQuery struct{}

type WeightedReceiverResponse struct {
	Data WeightedReceiver `json:"data"`
}

type ClockQueryResponse struct {
	Data string `json:"data"`
}

type LpPositionQueryResponse struct {
	Data LpInfo `json:"data"`
}

// A query response from the Neutron contract. Note that when
// interchaintest returns query responses, it does so in the form
// `{"data": <RESPONSE>}`, so we need this outer data key, which is
// not present in the neutron contract, to properly deserialze.

type DepositorInterchainAccountAddressQueryResponse struct {
	DepositorInterchainAccountAddress string `json:"depositor_interchain_account_address"`
}

// astroport stableswap
type StableswapInstantiateMsg struct {
	TokenCodeId uint64      `json:"token_code_id"`
	FactoryAddr string      `json:"factory_addr"`
	AssetInfos  []AssetInfo `json:"asset_infos"`
	InitParams  []byte      `json:"init_params"`
}

type AssetInfo struct {
	Token       *Token       `json:"token,omitempty"`
	NativeToken *NativeToken `json:"native_token,omitempty"`
}

type StablePoolParams struct {
	Amp   uint64  `json:"amp"`
	Owner *string `json:"owner"`
}

type Token struct {
	ContractAddr string `json:"contract_addr"`
}

type NativeToken struct {
	Denom string `json:"denom"`
}

// astroport factory
type FactoryInstantiateMsg struct {
	PairConfigs         []PairConfig `json:"pair_configs"`
	TokenCodeId         uint64       `json:"token_code_id"`
	FeeAddress          *string      `json:"fee_address"`
	GeneratorAddress    *string      `json:"generator_address"`
	Owner               string       `json:"owner"`
	WhitelistCodeId     uint64       `json:"whitelist_code_id"`
	CoinRegistryAddress string       `json:"coin_registry_address"`
}

type PairConfig struct {
	CodeId              uint64   `json:"code_id"`
	PairType            PairType `json:"pair_type"`
	TotalFeeBps         uint64   `json:"total_fee_bps"`
	MakerFeeBps         uint64   `json:"maker_fee_bps"`
	IsDisabled          bool     `json:"is_disabled"`
	IsGeneratorDisabled bool     `json:"is_generator_disabled"`
}

type PairType struct {
	// Xyk    struct{} `json:"xyk,omitempty"`
	Stable struct{} `json:"stable,omitempty"`
	// Custom struct{} `json:"custom,omitempty"`
}

// astroport native coin registry

type NativeCoinRegistryInstantiateMsg struct {
	Owner string `json:"owner"`
}

type AddExecuteMsg struct {
	Add Add `json:"add"`
}

type Add struct {
	NativeCoins []NativeCoin `json:"native_coins"`
}

type NativeCoin struct {
	Name  string `json:"name"`
	Value uint8  `json:"value"`
}

// Add { native_coins: Vec<(String, u8)> },

// astroport native token
type NativeTokenInstantiateMsg struct {
	Name            string                    `json:"name"`
	Symbol          string                    `json:"symbol"`
	Decimals        uint8                     `json:"decimals"`
	InitialBalances []Cw20Coin                `json:"initial_balances"`
	Mint            *MinterResponse           `json:"mint"`
	Marketing       *InstantiateMarketingInfo `json:"marketing"`
}

type Cw20Coin struct {
	Address string `json:"address"`
	Amount  uint64 `json:"amount"`
}

type MinterResponse struct {
	Minter string  `json:"minter"`
	Cap    *uint64 `json:"cap,omitempty"`
}

type InstantiateMarketingInfo struct {
	Project     string `json:"project"`
	Description string `json:"description"`
	Marketing   string `json:"marketing"`
	Logo        Logo   `json:"logo"`
}

type Logo struct {
	Url string `json:"url"`
}

// astroport whitelist
type WhitelistInstantiateMsg struct {
	Admins  []string `json:"admins"`
	Mutable bool     `json:"mutable"`
}

// ls
type LsInstantiateMsg struct {
	AutopilotPosition                 string `json:"autopilot_position,string"`
	ClockAddress                      string `json:"clock_address,string"`
	StrideNeutronIBCTransferChannelId string `json:"stride_neutron_ibc_transfer_channel_id"`
	LpAddress                         string `json:"lp_address"`
	NeutronStrideIBCConnectionId      string `json:"neutron_stride_ibc_connection_id"`
}
