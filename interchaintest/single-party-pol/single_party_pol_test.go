package covenant_single_party_pol

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/cosmos/cosmos-sdk/crypto/keyring"
	ibctest "github.com/strangelove-ventures/interchaintest/v4"
	"github.com/strangelove-ventures/interchaintest/v4/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v4/ibc"
	"github.com/strangelove-ventures/interchaintest/v4/relayer"
	"github.com/strangelove-ventures/interchaintest/v4/testreporter"
	"github.com/strangelove-ventures/interchaintest/v4/testutil"
	"github.com/stretchr/testify/require"
	utils "github.com/timewave-computer/covenants/interchaintest/utils"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"
)

const gaiaNeutronICSPath = "gn-ics-path"
const gaiaNeutronIBCPath = "gn-ibc-path"
const gaiaStrideIBCPath = "go-ibc-path"
const neutronStrideIBCPath = "no-ibc-path"
const nativeAtomDenom = "uatom"
const nativeStrideAtomDenom = "stuatom"
const nativeNtrnDenom = "untrn"

var covenantAddress string
var clockAddress string
var liquidPoolerAddress string
var partyDepositAddress string
var holderAddress string
var liquidStakerAddress string
var lsForwarderAddress string
var remoteChainSplitterAddress string
var liquidPoolerForwarderAddress string
var strideIcaAddress string
var lsForwarderIcaAddress, liquidPoolerForwarderIcaAddress string

var neutronAtomIbcDenom, neutronStatomIbcDenom, strideAtomIbcDenom string
var atomNeutronICSConnectionId, neutronAtomICSConnectionId string
var neutronStrideIBCConnId, strideNeutronIBCConnId string
var atomNeutronIBCConnId, neutronAtomIBCConnId string
var atomStrideIBCConnId, strideAtomIBCConnId string
var gaiaStrideIBCConnId, strideGaiaIBCConnId string
var tokenAddress string
var whitelistAddress string
var factoryAddress string
var coinRegistryAddress string
var stableswapAddress string
var liquidityTokenAddress string

// PARTY_A
const atomContributionAmount uint64 = 5_000_000_000 // in uatom

// sets up and tests a single party pol by hub
func TestSinglePartyPol(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping in short mode")
	}

	os.Setenv("IBCTEST_CONFIGURED_CHAINS", "./chains.yaml")

	ctx := context.Background()

	// Modify the the timeout_commit in the config.toml node files
	// to reduce the block commit times. This speeds up the tests
	// by about 35%
	configFileOverrides := make(map[string]any)
	configTomlOverrides := make(testutil.Toml)
	consensus := make(testutil.Toml)
	consensus["timeout_commit"] = "1s"
	configTomlOverrides["consensus"] = consensus
	configFileOverrides["config/config.toml"] = configTomlOverrides

	// Chain Factory
	cf := ibctest.NewBuiltinChainFactory(zaptest.NewLogger(t, zaptest.Level(zap.WarnLevel)), []*ibctest.ChainSpec{
		{Name: "gaia", Version: "v9.1.0", ChainConfig: ibc.ChainConfig{
			GasAdjustment:       1.3,
			GasPrices:           "0.0atom",
			ModifyGenesis:       utils.SetupGaiaGenesis(utils.GetDefaultInterchainGenesisMessages()),
			ConfigFileOverrides: configFileOverrides,
		}},
		{
			ChainConfig: ibc.ChainConfig{
				Type:    "cosmos",
				Name:    "neutron",
				ChainID: "neutron-2",
				Images: []ibc.DockerImage{
					{
						Repository: "ghcr.io/strangelove-ventures/heighliner/neutron",
						Version:    "v2.0.0",
						UidGid:     "1025:1025",
					},
				},
				Bin:            "neutrond",
				Bech32Prefix:   "neutron",
				Denom:          nativeNtrnDenom,
				GasPrices:      "0.0untrn,0.0uatom",
				GasAdjustment:  1.3,
				TrustingPeriod: "1197504s",
				NoHostMount:    false,
				ModifyGenesis: utils.SetupNeutronGenesis(
					"0.05",
					[]string{nativeNtrnDenom},
					[]string{nativeAtomDenom},
					utils.GetDefaultNeutronInterchainGenesisMessages(),
				),
				ConfigFileOverrides: configFileOverrides,
			},
		},
		{
			ChainConfig: ibc.ChainConfig{
				Type:    "cosmos",
				Name:    "stride",
				ChainID: "stride-3",
				Images: []ibc.DockerImage{
					{
						Repository: "stride",
						Version:    "non-ics",
						UidGid:     "1025:1025",
					},
				},
				Bin:          "strided",
				Bech32Prefix: "stride",
				Denom:        "ustrd",
				ModifyGenesis: utils.SetupStrideGenesis([]string{
					"/cosmos.bank.v1beta1.MsgSend",
					"/cosmos.bank.v1beta1.MsgMultiSend",
					"/cosmos.staking.v1beta1.MsgDelegate",
					"/cosmos.staking.v1beta1.MsgUndelegate",
					"/cosmos.staking.v1beta1.MsgBeginRedelegate",
					"/cosmos.staking.v1beta1.MsgRedeemTokensforShares",
					"/cosmos.staking.v1beta1.MsgTokenizeShares",
					"/cosmos.distribution.v1beta1.MsgWithdrawDelegatorReward",
					"/cosmos.distribution.v1beta1.MsgSetWithdrawAddress",
					"/ibc.applications.transfer.v1.MsgTransfer",
				}),
				GasPrices:           "0.0ustrd",
				GasAdjustment:       1.3,
				TrustingPeriod:      "336h",
				NoHostMount:         false,
				ConfigFileOverrides: configFileOverrides,
			},
		},
	})

	chains, err := cf.Chains(t.Name())
	require.NoError(t, err)

	// We have three chains
	atom, neutron, stride := chains[0], chains[1], chains[2]
	cosmosAtom, cosmosNeutron, cosmosStride := atom.(*cosmos.CosmosChain), neutron.(*cosmos.CosmosChain), stride.(*cosmos.CosmosChain)

	// Relayer Factory
	client, network := ibctest.DockerSetup(t)
	r := ibctest.NewBuiltinRelayerFactory(
		ibc.CosmosRly,
		zaptest.NewLogger(t, zaptest.Level(zap.InfoLevel)),
		relayer.CustomDockerImage("ghcr.io/cosmos/relayer", "v2.4.0", "1000:1000"),
		relayer.RelayerOptionExtraStartFlags{Flags: []string{"-p", "events", "-b", "100", "-d", "--log-format", "console"}},
	).Build(t, client, network)

	// Prep Interchain
	ic := ibctest.NewInterchain().
		AddChain(cosmosAtom).
		AddChain(cosmosNeutron).
		AddChain(cosmosStride).
		AddRelayer(r, "relayer").
		AddProviderConsumerLink(ibctest.ProviderConsumerLink{
			Provider: cosmosAtom,
			Consumer: cosmosNeutron,
			Relayer:  r,
			Path:     gaiaNeutronICSPath,
		}).
		AddLink(ibctest.InterchainLink{
			Chain1:  cosmosAtom,
			Chain2:  cosmosNeutron,
			Relayer: r,
			Path:    gaiaNeutronIBCPath,
		}).
		AddLink(ibctest.InterchainLink{
			Chain1:  cosmosNeutron,
			Chain2:  cosmosStride,
			Relayer: r,
			Path:    neutronStrideIBCPath,
		}).
		AddLink(ibctest.InterchainLink{
			Chain1:  cosmosAtom,
			Chain2:  cosmosStride,
			Relayer: r,
			Path:    gaiaStrideIBCPath,
		})

	// Log location
	f, err := ibctest.CreateLogFile(fmt.Sprintf("%d.json", time.Now().Unix()))
	require.NoError(t, err)
	// Reporter/logs
	rep := testreporter.NewReporter(f)
	eRep := rep.RelayerExecReporter(t)

	// Build interchain
	require.NoError(
		t,
		ic.Build(ctx, eRep, ibctest.InterchainBuildOptions{
			TestName:          t.Name(),
			Client:            client,
			NetworkID:         network,
			BlockDatabaseFile: ibctest.DefaultBlockDatabaseFilepath(),
			SkipPathCreation:  true,
		}),
		"failed to build interchain")

	testCtx := &utils.TestContext{
		Neutron:                   cosmosNeutron,
		Hub:                       cosmosAtom,
		Stride:                    cosmosStride,
		StrideClients:             []*ibc.ClientOutput{},
		GaiaClients:               []*ibc.ClientOutput{},
		NeutronClients:            []*ibc.ClientOutput{},
		StrideConnections:         []*ibc.ConnectionOutput{},
		GaiaConnections:           []*ibc.ConnectionOutput{},
		NeutronConnections:        []*ibc.ConnectionOutput{},
		NeutronTransferChannelIds: make(map[string]string),
		GaiaTransferChannelIds:    make(map[string]string),
		StrideTransferChannelIds:  make(map[string]string),
		GaiaIcsChannelIds:         make(map[string]string),
		NeutronIcsChannelIds:      make(map[string]string),
		T:                         t,
		Ctx:                       ctx,
	}

	testCtx.SkipBlocksStride(5)

	t.Run("generate IBC paths", func(t *testing.T) {
		utils.GeneratePath(t, ctx, r, eRep, cosmosAtom.Config().ChainID, cosmosNeutron.Config().ChainID, gaiaNeutronIBCPath)
		utils.GeneratePath(t, ctx, r, eRep, cosmosAtom.Config().ChainID, cosmosStride.Config().ChainID, gaiaStrideIBCPath)
		utils.GeneratePath(t, ctx, r, eRep, cosmosNeutron.Config().ChainID, cosmosStride.Config().ChainID, neutronStrideIBCPath)
		utils.GeneratePath(t, ctx, r, eRep, cosmosNeutron.Config().ChainID, cosmosAtom.Config().ChainID, gaiaNeutronICSPath)
	})

	t.Run("setup neutron-gaia ICS", func(t *testing.T) {
		utils.GenerateClient(t, ctx, testCtx, r, eRep, gaiaNeutronICSPath, cosmosAtom, cosmosNeutron)
		neutronClients := testCtx.GetChainClients(cosmosNeutron.Config().Name)
		atomClients := testCtx.GetChainClients(cosmosAtom.Config().Name)

		err = r.UpdatePath(ctx, eRep, gaiaNeutronICSPath, ibc.PathUpdateOptions{
			SrcClientID: &neutronClients[0].ClientID,
			DstClientID: &atomClients[0].ClientID,
		})
		require.NoError(t, err)

		atomNeutronICSConnectionId, neutronAtomICSConnectionId = utils.GenerateConnections(t, ctx, testCtx, r, eRep, gaiaNeutronICSPath, cosmosAtom, cosmosNeutron)

		utils.GenerateICSChannel(t, ctx, r, eRep, gaiaNeutronICSPath, cosmosAtom, cosmosNeutron)

		utils.CreateValidator(t, ctx, r, eRep, atom, neutron)
		testCtx.SkipBlocksStride(2)
	})

	t.Run("setup IBC interchain clients, connections, and links", func(t *testing.T) {
		utils.GenerateClient(t, ctx, testCtx, r, eRep, neutronStrideIBCPath, cosmosNeutron, cosmosStride)
		neutronStrideIBCConnId, strideNeutronIBCConnId = utils.GenerateConnections(t, ctx, testCtx, r, eRep, neutronStrideIBCPath, cosmosNeutron, cosmosStride)
		utils.LinkPath(t, ctx, r, eRep, cosmosNeutron, cosmosStride, neutronStrideIBCPath)

		utils.GenerateClient(t, ctx, testCtx, r, eRep, gaiaStrideIBCPath, cosmosAtom, cosmosStride)
		gaiaStrideIBCConnId, strideGaiaIBCConnId = utils.GenerateConnections(t, ctx, testCtx, r, eRep, gaiaStrideIBCPath, cosmosAtom, cosmosStride)
		utils.LinkPath(t, ctx, r, eRep, cosmosAtom, cosmosStride, gaiaStrideIBCPath)

		utils.GenerateClient(t, ctx, testCtx, r, eRep, gaiaNeutronIBCPath, cosmosAtom, cosmosNeutron)
		atomNeutronIBCConnId, neutronAtomIBCConnId = utils.GenerateConnections(t, ctx, testCtx, r, eRep, gaiaNeutronIBCPath, cosmosAtom, cosmosNeutron)
		utils.LinkPath(t, ctx, r, eRep, cosmosAtom, cosmosNeutron, gaiaNeutronIBCPath)
	})

	// Start the relayer and clean it up when the test ends.
	err = r.StartRelayer(ctx, eRep, gaiaNeutronICSPath, gaiaNeutronIBCPath, gaiaStrideIBCPath, neutronStrideIBCPath)
	require.NoError(t, err, "failed to start relayer with given paths")
	t.Cleanup(func() {
		err = r.StopRelayer(ctx, eRep)
		if err != nil {
			t.Logf("failed to stop relayer: %s", err)
		}
	})
	testCtx.SkipBlocksStride(2)

	// Once the VSC packet has been relayed, x/bank transfers are
	// enabled on Neutron and we can fund its account.
	// The funds for this are sent from a "faucet" account created
	// by interchaintest in the genesis file.
	users := ibctest.GetAndFundTestUsers(t, ctx, "default", int64(500_000_000_000), atom, neutron, stride)
	gaiaUser, neutronUser, strideUser := users[0], users[1], users[2]
	_, _, _ = gaiaUser, neutronUser, strideUser

	strideAdminMnemonic := "tone cause tribe this switch near host damage idle fragile antique tail soda alien depth write wool they rapid unfold body scan pledge soft"
	strideAdmin, _ := ibctest.GetAndFundTestUserWithMnemonic(ctx, "default", strideAdminMnemonic, (100_000_000), cosmosStride)

	cosmosStride.SendFunds(ctx, strideUser.KeyName, ibc.WalletAmount{
		Address: strideAdmin.Bech32Address(stride.Config().Bech32Prefix),
		Denom:   "ustrd",
		Amount:  10000000,
	})

	testCtx.SkipBlocksStride(5)

	t.Run("determine ibc channels", func(t *testing.T) {
		neutronChannelInfo, _ := r.GetChannels(ctx, eRep, cosmosNeutron.Config().ChainID)
		gaiaChannelInfo, _ := r.GetChannels(ctx, eRep, cosmosAtom.Config().ChainID)
		strideChannelInfo, _ := r.GetChannels(ctx, eRep, cosmosStride.Config().ChainID)

		// Find all pairwise channels
		utils.GetPairwiseTransferChannelIds(testCtx, strideChannelInfo, neutronChannelInfo, strideNeutronIBCConnId, neutronStrideIBCConnId, stride.Config().Name, neutron.Config().Name)
		utils.GetPairwiseTransferChannelIds(testCtx, strideChannelInfo, gaiaChannelInfo, strideGaiaIBCConnId, gaiaStrideIBCConnId, stride.Config().Name, cosmosAtom.Config().Name)
		utils.GetPairwiseTransferChannelIds(testCtx, gaiaChannelInfo, neutronChannelInfo, atomNeutronIBCConnId, neutronAtomIBCConnId, cosmosAtom.Config().Name, neutron.Config().Name)
		utils.GetPairwiseCCVChannelIds(testCtx, gaiaChannelInfo, neutronChannelInfo, atomNeutronICSConnectionId, neutronAtomICSConnectionId, cosmosAtom.Config().Name, cosmosNeutron.Config().Name)
	})

	t.Run("determine ibc denoms", func(t *testing.T) {
		// We can determine the ibc denoms of:
		// 1. ATOM on Neutron
		neutronAtomIbcDenom = testCtx.GetIbcDenom(
			testCtx.NeutronTransferChannelIds[cosmosAtom.Config().Name],
			nativeAtomDenom,
		)
		// 2. statom on neutron
		neutronStatomIbcDenom = testCtx.GetIbcDenom(
			testCtx.NeutronTransferChannelIds[cosmosStride.Config().Name],
			nativeStrideAtomDenom,
		)
		// 3. atom on stride
		strideAtomIbcDenom = testCtx.GetIbcDenom(
			testCtx.StrideTransferChannelIds[cosmosAtom.Config().Name],
			nativeAtomDenom,
		)
	})

	// Stride is a liquid staking platform. We need to register Gaia (ATOM)
	// as a host zone in order to redeem stATOM in exchange for ATOM
	// stATOM is stride's liquid staked ATOM vouchers.
	t.Run("register stride host zone", func(t *testing.T) {

		cmd := []string{"strided", "tx", "stakeibc", "register-host-zone",
			strideGaiaIBCConnId,
			cosmosAtom.Config().Denom,
			cosmosAtom.Config().Bech32Prefix,
			strideAtomIbcDenom,
			testCtx.StrideTransferChannelIds[cosmosAtom.Config().Name],
			"1",
			"--from", strideAdmin.KeyName,
			"--gas", "auto",
			"--gas-adjustment", `1.3`,
			"--output", "json",
			"--chain-id", cosmosStride.Config().ChainID,
			"--node", cosmosStride.GetRPCAddress(),
			"--home", cosmosStride.HomeDir(),
			"--keyring-backend", keyring.BackendTest,
			"-y",
		}

		_, _, err = cosmosStride.Exec(ctx, cmd, nil)
		require.NoError(t, err, "failed to register host zone on stride")

		testCtx.SkipBlocksStride(8)
	})
	// Stride needs validators that it can stake ATOM with to issue us stATOM
	t.Run("register gaia validators on stride", func(t *testing.T) {

		type Validator struct {
			Name    string `json:"name"`
			Address string `json:"address"`
			Weight  int    `json:"weight"`
		}

		type Data struct {
			BlockHeight string      `json:"block_height"`
			Total       string      `json:"total"`
			Validators  []Validator `json:"validators"`
		}

		valcmd := []string{"gaiad", "query", "tendermint-validator-set",
			"50",
			"--chain-id", cosmosAtom.Config().ChainID,
			"--node", cosmosAtom.GetRPCAddress(),
			"--home", cosmosAtom.HomeDir(),
		}
		resp, _, err := cosmosAtom.Exec(ctx, valcmd, nil)
		require.NoError(t, err, "Failed to query valset")
		testCtx.SkipBlocksStride(2)

		var addresses []string
		var votingPowers []string

		lines := strings.Split(string(resp), "\n")

		for _, line := range lines {
			if strings.HasPrefix(line, "- address: ") {
				address := strings.TrimPrefix(line, "- address: ")
				addresses = append(addresses, address)
			} else if strings.HasPrefix(line, "  voting_power: ") {
				votingPower := strings.TrimPrefix(line, "  voting_power: ")
				votingPowers = append(votingPowers, votingPower)
			}
		}

		// Create validators slice
		var validators []Validator

		for i := 1; i <= len(addresses); i++ {
			votingPowStr := strings.ReplaceAll(votingPowers[i-1], "\"", "")
			valWeight, err := strconv.Atoi(votingPowStr)
			require.NoError(t, err, "failed to parse voting power")

			validator := Validator{
				Name:    fmt.Sprintf("val%d", i),
				Address: addresses[i-1],
				Weight:  valWeight,
			}
			validators = append(validators, validator)
		}

		// Create JSON object
		data := map[string][]Validator{
			"validators": validators,
		}

		// Convert to JSON
		jsonData, err := json.Marshal(data)
		require.NoError(t, err, "failed to marshall data")

		fullPath := filepath.Join(cosmosStride.HomeDir(), "vals.json")
		bashCommand := "echo '" + string(jsonData) + "' > " + fullPath
		fullPathCmd := []string{"/bin/sh", "-c", bashCommand}

		_, _, err = cosmosStride.Exec(ctx, fullPathCmd, nil)
		require.NoError(t, err, "failed to create json with gaia LS validator set on stride")
		testCtx.SkipBlocksStride(5)

		cmd := []string{"strided", "tx", "stakeibc", "add-validators",
			cosmosAtom.Config().ChainID,
			fullPath,
			"--from", strideAdmin.KeyName,
			"--gas", "auto",
			"--gas-adjustment", `1.3`,
			"--output", "json",
			"--chain-id", cosmosStride.Config().ChainID,
			"--node", cosmosStride.GetRPCAddress(),
			"--home", cosmosStride.HomeDir(),
			"--keyring-backend", keyring.BackendTest,
			"-y",
		}

		_, _, err = cosmosStride.Exec(ctx, cmd, nil)
		require.NoError(t, err, "failed to register host zone on stride")

		testCtx.SkipBlocksStride(5)

		queryCmd := []string{"strided", "query", "stakeibc",
			"show-validators",
			cosmosAtom.Config().ChainID,
			"--chain-id", cosmosStride.Config().ChainID,
			"--node", cosmosStride.GetRPCAddress(),
			"--home", cosmosStride.HomeDir(),
		}

		_, _, err = cosmosStride.Exec(ctx, queryCmd, nil)
		require.NoError(t, err, "failed to query host validators")
	})

	t.Run("two party pol covenant setup", func(t *testing.T) {
		// Wasm code that we need to store on Neutron
		const covenantContractPath = "wasms/covenant_single_party_pol.wasm"
		const clockContractPath = "wasms/covenant_clock.wasm"
		const interchainRouterContractPath = "wasms/covenant_interchain_router.wasm"
		const nativeRouterContractPath = "wasms/covenant_native_router.wasm"
		const ibcForwarderContractPath = "wasms/covenant_ibc_forwarder.wasm"
		const holderContractPath = "wasms/covenant_single_party_pol_holder.wasm"
		const liquidPoolerPath = "wasms/covenant_astroport_liquid_pooler.wasm"
		const remoteChainSplitterPath = "wasms/covenant_native_splitter.wasm"
		const liquidStakerContractPath = "wasms/covenant_stride_liquid_staker.wasm"

		// After storing on Neutron, we will receive a code id
		// We parse all the subcontracts into uint64
		// The will be required when we instantiate the covenant.
		var clockCodeId uint64
		var interchainRouterCodeId uint64
		var nativeRouterCodeId uint64
		var ibcForwarderCodeId uint64
		var holderCodeId uint64
		var lperCodeId uint64
		var liquidStakerCodeId uint64
		var covenantCodeId uint64
		var remoteChainSplitterCodeId uint64
		_, _, _, _, _, _, _, _, _ = clockCodeId, interchainRouterCodeId, nativeRouterCodeId, ibcForwarderCodeId, holderCodeId, lperCodeId, covenantCodeId, remoteChainSplitterCodeId, liquidStakerCodeId

		t.Run("deploy covenant contracts", func(t *testing.T) {
			covenantCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, covenantContractPath)

			// store clock and get code id
			clockCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, clockContractPath)

			// store routers and get code id
			interchainRouterCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, interchainRouterContractPath)
			nativeRouterCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, nativeRouterContractPath)

			// store forwarder and get code id
			ibcForwarderCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, ibcForwarderContractPath)

			// store lper, get code
			lperCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, liquidPoolerPath)

			// store holder and get code id
			holderCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, holderContractPath)

			liquidStakerCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, liquidStakerContractPath)
			// store remote chain splitter and get code id
			remoteChainSplitterCodeId = testCtx.StoreContract(cosmosNeutron, neutronUser, remoteChainSplitterPath)

			testCtx.SkipBlocksStride(5)
		})

		t.Run("deploy astroport contracts", func(t *testing.T) {
			stablePairCodeId := testCtx.StoreContract(cosmosNeutron, neutronUser, "wasms/astroport_pair_stable.wasm")
			factoryCodeId := testCtx.StoreContract(cosmosNeutron, neutronUser, "wasms/astroport_factory.wasm")
			whitelistCodeId := testCtx.StoreContract(cosmosNeutron, neutronUser, "wasms/astroport_whitelist.wasm")
			tokenCodeId := testCtx.StoreContract(cosmosNeutron, neutronUser, "wasms/astroport_token.wasm")
			coinRegistryCodeId := testCtx.StoreContract(cosmosNeutron, neutronUser, "wasms/astroport_native_coin_registry.wasm")

			t.Run("astroport token", func(t *testing.T) {
				msg := NativeTokenInstantiateMsg{
					Name:            "nativetoken",
					Symbol:          "ntk",
					Decimals:        5,
					InitialBalances: []Cw20Coin{},
					Mint:            nil,
					Marketing:       nil,
				}
				str, _ := json.Marshal(msg)

				tokenAddress, err = cosmosNeutron.InstantiateContract(
					ctx, neutronUser.KeyName, strconv.FormatUint(tokenCodeId, 10), string(str), true)
				require.NoError(t, err, "Failed to instantiate nativetoken")
				println("astroport token: ", tokenAddress)
			})

			t.Run("whitelist", func(t *testing.T) {
				msg := WhitelistInstantiateMsg{
					Admins:  []string{neutronUser.Bech32Address(neutron.Config().Bech32Prefix)},
					Mutable: false,
				}
				str, _ := json.Marshal(msg)

				whitelistAddress, err = cosmosNeutron.InstantiateContract(
					ctx, neutronUser.KeyName, strconv.FormatUint(whitelistCodeId, 10), string(str), true)
				require.NoError(t, err, "Failed to instantiate Whitelist")
				println("astroport whitelist: ", whitelistAddress)

			})

			t.Run("native coins registry", func(t *testing.T) {
				msg := NativeCoinRegistryInstantiateMsg{
					Owner: neutronUser.Bech32Address(neutron.Config().Bech32Prefix),
				}
				str, _ := json.Marshal(msg)

				nativeCoinRegistryAddress, err := cosmosNeutron.InstantiateContract(
					ctx, neutronUser.KeyName, strconv.FormatUint(coinRegistryCodeId, 10), string(str), true)
				require.NoError(t, err, "Failed to instantiate NativeCoinRegistry")
				coinRegistryAddress = nativeCoinRegistryAddress
				println("astroport native coins registry: ", coinRegistryAddress)
			})

			t.Run("add coins to registry", func(t *testing.T) {
				// Add ibc native tokens for statom and uatom to the native coin registry
				// each of these tokens has a precision of 6
				addMessage := fmt.Sprintf(
					`{"add":{"native_coins":[["%s",6],["%s",6]]}}`,
					neutronAtomIbcDenom,
					neutronStatomIbcDenom)
				_, err = cosmosNeutron.ExecuteContract(ctx, neutronUser.KeyName, coinRegistryAddress, addMessage)
				require.NoError(t, err, err)
				testCtx.SkipBlocksStride(2)
			})

			t.Run("factory", func(t *testing.T) {
				factoryAddress = testCtx.InstantiateAstroportFactory(
					stablePairCodeId, tokenCodeId, whitelistCodeId, factoryCodeId, coinRegistryAddress, neutronUser)
				println("astroport factory: ", factoryAddress)
				testCtx.SkipBlocksStride(2)
			})

			t.Run("create pair on factory", func(t *testing.T) {
				testCtx.CreateAstroportFactoryPairStride(3, neutronStatomIbcDenom, neutronAtomIbcDenom, factoryAddress, neutronUser, keyring.BackendTest)
			})
		})

		t.Run("fund stride user with atom to liquidstake", func(t *testing.T) {

			autopilotString := `{"autopilot":{"receiver":"` + strideUser.Bech32Address(stride.Config().Bech32Prefix) + `","stakeibc":{"stride_address":"` + strideUser.Bech32Address(stride.Config().Bech32Prefix) + `","action":"LiquidStake"}}}`
			cmd := []string{cosmosAtom.Config().Bin, "tx", "ibc-transfer", "transfer", "transfer",
				testCtx.GaiaTransferChannelIds[cosmosStride.Config().Name], autopilotString,
				"100000000000uatom",
				"--keyring-backend", keyring.BackendTest,
				"--node", cosmosAtom.GetRPCAddress(),
				"--from", gaiaUser.KeyName,
				"--gas", "auto",
				"--home", cosmosAtom.HomeDir(),
				"--chain-id", cosmosAtom.Config().ChainID,
				"-y",
			}
			_, _, err = cosmosAtom.Exec(ctx, cmd, nil)
			require.NoError(t, err)

			testCtx.SkipBlocksStride(10)

			// ibc transfer statom on stride to neutron user
			transferStAtomNeutron := ibc.WalletAmount{
				Address: neutronUser.Bech32Address(neutron.Config().Bech32Prefix),
				Denom:   "stuatom",
				Amount:  int64(100000000000),
			}
			_, err = cosmosStride.SendIBCTransfer(ctx, testCtx.StrideTransferChannelIds[cosmosNeutron.Config().Name], strideUser.KeyName, transferStAtomNeutron, ibc.TransferOptions{})
			require.NoError(t, err)

			testCtx.SkipBlocksStride(10)
		})

		t.Run("add liquidity to the atom-statom stableswap pool", func(t *testing.T) {
			liquidityTokenAddress, stableswapAddress = testCtx.QueryAstroLpTokenAndStableswapAddress(
				factoryAddress, neutronStatomIbcDenom, neutronAtomIbcDenom)
			// set up the pool with 1:10 ratio of atom/statom
			_, err := atom.SendIBCTransfer(ctx,
				testCtx.GaiaTransferChannelIds[cosmosNeutron.Config().Name],
				gaiaUser.KeyName,
				ibc.WalletAmount{
					Address: neutronUser.Bech32Address(neutron.Config().Bech32Prefix),
					Denom:   cosmosAtom.Config().Denom,
					Amount:  int64(atomContributionAmount),
				},
				ibc.TransferOptions{})
			require.NoError(t, err)

			testCtx.SkipBlocksStride(2)

			testCtx.ProvideAstroportLiquidity(
				neutronAtomIbcDenom, neutronStatomIbcDenom, atomContributionAmount/2, atomContributionAmount/2, neutronUser, stableswapAddress)

			testCtx.SkipBlocksStride(2)
			neutronUserLPTokenBal := testCtx.QueryLpTokenBalance(liquidityTokenAddress, neutronUser.Bech32Address(neutron.Config().Bech32Prefix))
			println("neutronUser lp token bal: ", neutronUserLPTokenBal)
		})

		t.Run("init covenant", func(t *testing.T) {
			presetIbcFee := PresetIbcFee{
				AckFee:     "100000",
				TimeoutFee: "100000",
			}

			timeouts := Timeouts{
				IcaTimeout:         "10000", // sec
				IbcTransferTimeout: "10000", // sec
			}

			contractCodes := ContractCodeIds{
				IbcForwarderCode:   ibcForwarderCodeId,
				ClockCode:          clockCodeId,
				HolderCode:         holderCodeId,
				LiquidPoolerCode:   lperCodeId,
				LiquidStakerCode:   liquidStakerCodeId,
				NativeSplitterCode: remoteChainSplitterCodeId,
			}
			currentHeight := testCtx.GetNeutronHeight()

			lockupBlock := Block(currentHeight + 110)
			lockupConfig := Expiration{
				AtHeight: &lockupBlock,
			}

			lsInfo := LsInfo{
				LsDenom:                   "stuatom",
				LsDenomOnNeutron:          neutronStatomIbcDenom,
				LsChainToNeutronChannelId: testCtx.StrideTransferChannelIds[testCtx.Neutron.Config().Name],
				LsNeutronConnectionId:     neutronStrideIBCConnId,
			}

			lsContribution := Coin{
				Denom:  nativeAtomDenom,
				Amount: "2500000000",
			}
			liquidPoolerContribution := Coin{
				Denom:  nativeAtomDenom,
				Amount: "2500000000",
			}

			// here the remote chain info should define gaia -> stride flow
			// source channel = gaia to stride chan
			// receiver = liquid staker stride ICA
			// connection id = neutron-gaia connection (for ica to be on gaia)

			lsForwarderConfig := CovenantPartyConfig{
				Interchain: &InterchainCovenantParty{
					Addr:                      neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix),
					NativeDenom:               neutronAtomIbcDenom,
					RemoteChainDenom:          "uatom",
					PartyToHostChainChannelId: testCtx.GaiaTransferChannelIds[cosmosStride.Config().Name],
					HostToPartyChainChannelId: testCtx.StrideTransferChannelIds[cosmosAtom.Config().Name],
					PartyReceiverAddr:         neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix),
					PartyChainConnectionId:    neutronAtomIBCConnId,
					IbcTransferTimeout:        timeouts.IbcTransferTimeout,
					Contribution:              lsContribution,
				},
			}

			liquidPoolerForwarderConfig := CovenantPartyConfig{
				Interchain: &InterchainCovenantParty{
					Addr:                      neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix),
					NativeDenom:               neutronAtomIbcDenom,
					RemoteChainDenom:          "uatom",
					PartyToHostChainChannelId: testCtx.GaiaTransferChannelIds[cosmosNeutron.Config().Name],
					HostToPartyChainChannelId: testCtx.NeutronTransferChannelIds[cosmosAtom.Config().Name],
					PartyReceiverAddr:         neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix),
					PartyChainConnectionId:    neutronAtomIBCConnId,
					IbcTransferTimeout:        timeouts.IbcTransferTimeout,
					Contribution:              liquidPoolerContribution,
				},
			}

			pairType := PairType{
				Stable: struct{}{},
			}

			nativeSplitterConfig := NativeSplitterConfig{
				ChannelId:    testCtx.NeutronTransferChannelIds[cosmosAtom.Config().Name],
				ConnectionId: neutronAtomIBCConnId,
				Denom:        nativeAtomDenom,
				Amount:       strconv.FormatUint(atomContributionAmount, 10),
			}

			covenantInstantiationMsg := CovenantInstantiationMsg{
				Label:                    "single_party_pol_covenant",
				Timeouts:                 timeouts,
				PresetIbcFee:             presetIbcFee,
				ContractCodeIds:          contractCodes,
				LockupConfig:             lockupConfig,
				PoolAddress:              stableswapAddress,
				LsInfo:                   lsInfo,
				PartyASingleSideLimit:    "10000000",
				PartyBSingleSideLimit:    "10000000",
				LsForwarderConfig:        lsForwarderConfig,
				HolderForwarderConfig:    liquidPoolerForwarderConfig,
				ExpectedPoolRatio:        "0.5",
				AcceptablePoolRatioDelta: "0.5",
				PairType:                 pairType,
				NativeSplitterConfig:     nativeSplitterConfig,
			}

			covenantAddress = testCtx.ManualInstantiateLS(covenantCodeId, covenantInstantiationMsg, neutronUser, keyring.BackendTest)
			println("covenant address: ", covenantAddress)
		})

		t.Run("query covenant contracts", func(t *testing.T) {
			clockAddress = testCtx.QueryClockAddress(covenantAddress)
			holderAddress = testCtx.QueryHolderAddress(covenantAddress)
			liquidPoolerAddress = testCtx.QueryLiquidPoolerAddress(covenantAddress)
			liquidStakerAddress = testCtx.QueryLiquidStakerAddress(covenantAddress)
			lsForwarderAddress = testCtx.QueryIbcForwarderTyAddress(covenantAddress, "ls")
			liquidPoolerForwarderAddress = testCtx.QueryIbcForwarderTyAddress(covenantAddress, "holder")
			remoteChainSplitterAddress = testCtx.QueryRemoteChainSplitterAddress(covenantAddress)
		})

		t.Run("fund contracts with neutron", func(t *testing.T) {
			addrs := []string{
				clockAddress,
				holderAddress,
				liquidPoolerAddress,
				liquidStakerAddress,
				lsForwarderAddress,
				liquidPoolerForwarderAddress,
				remoteChainSplitterAddress,
			}

			testCtx.FundChainAddrs(addrs, cosmosNeutron, neutronUser, 5000000000)
		})

		t.Run("tick until forwarders create ICA", func(t *testing.T) {
			for {
				testCtx.TickStride(clockAddress, keyring.BackendTest, neutronUser.KeyName)
				lsForwarderState := testCtx.QueryContractState(lsForwarderAddress)
				println("lsForwarderState: ", lsForwarderState)

				liquidPoolerForwarderState := testCtx.QueryContractState(liquidPoolerForwarderAddress)
				println("liquidPoolerForwarderState: ", liquidPoolerForwarderState)

				splitterState := testCtx.QueryContractState(remoteChainSplitterAddress)
				println("splitterState: ", splitterState)

				liquidStakerState := testCtx.QueryContractState(liquidStakerAddress)
				println("liquidStakerState: ", liquidStakerState)

				if splitterState == "ica_created" && lsForwarderState == "ica_created" && liquidPoolerForwarderState == "ica_created" && liquidStakerState == "ica_created" {
					partyDepositAddress = testCtx.QueryDepositAddressSingleParty(covenantAddress)
					strideIcaAddress = testCtx.QueryContractICA(liquidStakerAddress)
					lsForwarderIcaAddress = testCtx.QueryContractDepositAddress(lsForwarderAddress)
					liquidPoolerForwarderIcaAddress = testCtx.QueryContractDepositAddress(liquidPoolerForwarderAddress)
					println("ls forwarder ica address: ", lsForwarderIcaAddress)
					println("liquid pooler forwarder ica address: ", liquidPoolerForwarderIcaAddress)
					break
				}
			}
		})

		t.Run("fund the forwarders with sufficient funds", func(t *testing.T) {
			testCtx.FundChainAddrs([]string{partyDepositAddress}, cosmosAtom, gaiaUser, int64(atomContributionAmount))

			testCtx.SkipBlocksStride(3)
		})

		t.Run("tick until forwarders forward the funds to LPer", func(t *testing.T) {
			for {
				testCtx.TickStride(remoteChainSplitterAddress, keyring.BackendTest, neutronUser.KeyName)

				lsForwarderIcaAtomBal := testCtx.QueryHubDenomBalance("uatom", lsForwarderIcaAddress)
				liquidPoolerForwarderIcaAtomBal := testCtx.QueryHubDenomBalance("uatom", liquidPoolerForwarderIcaAddress)
				splitterAtomBalance := testCtx.QueryHubDenomBalance("uatom", partyDepositAddress)

				println("ls forwarder ica atom balance: ", lsForwarderIcaAtomBal)
				println("liquid pooler forwarder ica atom balance: ", liquidPoolerForwarderIcaAtomBal)
				println("splitter atom balance: ", splitterAtomBalance)

				if lsForwarderIcaAtomBal != 0 && liquidPoolerForwarderIcaAtomBal != 0 {
					println("liquid pooler received atom & statom")
					break
				}
			}
		})

		getLsPermisionlessTransferMsg := func(amount uint64) []string {
			// Construct a transfer message
			msg := TransferExecutionMsg{
				Transfer: TransferAmount{
					Amount: amount,
				},
			}
			transferMsgJson, err := json.Marshal(msg)
			require.NoError(t, err)

			// transfer command for permissionless transfer from stride ica to lper
			transferCmd := []string{"neutrond", "tx", "wasm", "execute", liquidStakerAddress,
				string(transferMsgJson),
				"--from", neutronUser.KeyName,
				"--gas-prices", "0.0untrn",
				"--gas-adjustment", `1.8`,
				"--output", "json",
				"--home", testCtx.Neutron.HomeDir(),
				"--node", testCtx.Neutron.GetRPCAddress(),
				"--chain-id", testCtx.Neutron.Config().ChainID,
				"--gas", "42069420",
				"--keyring-backend", keyring.BackendTest,
				"-y",
			}
			return transferCmd
		}
		var strideIcaStatomBal uint64

		t.Run("tick until LSer liquid stakes", func(t *testing.T) {
			for {
				testCtx.TickStride(clockAddress, keyring.BackendTest, neutronUser.KeyName)

				liquidPoolerStatomBal := testCtx.QueryNeutronDenomBalance(neutronStatomIbcDenom, liquidPoolerAddress)
				lperAtomBal := testCtx.QueryNeutronDenomBalance(neutronAtomIbcDenom, liquidPoolerAddress)
				strideIcaStatomBal = testCtx.QueryStrideDenomBalance("stuatom", strideIcaAddress)

				println("lper statom balance: ", liquidPoolerStatomBal)
				println("lper atom balance: ", lperAtomBal)
				println("stride ica statom balance: ", strideIcaStatomBal)

				if strideIcaStatomBal != 0 {
					println("stride ICA received statom: ", strideIcaStatomBal)
					break
				}
			}
		})

		t.Run("permisionless forward", func(t *testing.T) {
			testCtx.SkipBlocksStride(10)
			permisionlessTransferMsg := getLsPermisionlessTransferMsg(strideIcaStatomBal)
			txOut, _, _ := testCtx.Neutron.Exec(testCtx.Ctx, permisionlessTransferMsg, nil)
			println("permisionless transfer msg tx hash: ", string(txOut))
			testCtx.SkipBlocksStride(10)
		})

		t.Run("tick until LiquidPooler provides liquidity", func(t *testing.T) {
			for {
				liquidPoolerLpTokenBal := testCtx.QueryLpTokenBalance(liquidityTokenAddress, liquidPoolerAddress)
				holderLpTokenBal := testCtx.QueryLpTokenBalance(liquidityTokenAddress, holderAddress)
				neutronUserLpTokenBal := testCtx.QueryLpTokenBalance(liquidityTokenAddress, neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix))
				println("liquidPoolerLpTokenBal: ", liquidPoolerLpTokenBal)
				println("holderLpTokenBal: ", holderLpTokenBal)
				println("neutronUserLpTokenBal: ", neutronUserLpTokenBal)

				if liquidPoolerLpTokenBal == 0 {
					testCtx.TickStride(liquidPoolerAddress, keyring.BackendTest, neutronUser.KeyName)
				} else {
					break
				}
			}
		})

		t.Run("user redeems LP tokens for underlying liquidity", func(t *testing.T) {
			testCtx.SkipBlocksStride(10)
			// testCtx.HolderClaim(holderAddress, neutronUser, keyring.BackendTest)
			cmd := []string{"neutrond", "tx", "wasm", "execute", holderAddress,
				`{"claim":{}}`,
				"--from", neutronUser.GetKeyName(),
				"--gas-prices", "0.0untrn",
				"--gas-adjustment", `1.5`,
				"--output", "json",
				"--node", testCtx.Neutron.GetRPCAddress(),
				"--home", testCtx.Neutron.HomeDir(),
				"--chain-id", testCtx.Neutron.Config().ChainID,
				"--gas", "42069420",
				"--keyring-backend", keyring.BackendTest,
				"-y",
			}

			resp, _, err := testCtx.Neutron.Exec(testCtx.Ctx, cmd, nil)
			require.NoError(testCtx.T, err, "claim failed")
			println("claim response: ", string(resp))
			testCtx.SkipBlocksStride(5)

			for {
				stAtomBal := testCtx.QueryNeutronDenomBalance(neutronStatomIbcDenom, neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix))
				atomBal := testCtx.QueryNeutronDenomBalance(neutronAtomIbcDenom, neutronUser.Bech32Address(cosmosNeutron.Config().Bech32Prefix))
				println("neutron user stAtomBal: ", stAtomBal)
				println("neutron user atomBal: ", atomBal)
				if stAtomBal != 0 && atomBal != 0 {
					break
				} else {
					testCtx.Tick(clockAddress, keyring.BackendTest, neutronUser.KeyName)
				}
			}
		})
	})
}