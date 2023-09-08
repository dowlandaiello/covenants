package ibc_test

import (
	"context"
	"fmt"
	"testing"
	"time"

	ibctest "github.com/strangelove-ventures/interchaintest/v4"
	"github.com/strangelove-ventures/interchaintest/v4/chain/cosmos"
	"github.com/strangelove-ventures/interchaintest/v4/ibc"
	"github.com/strangelove-ventures/interchaintest/v4/relayer"
	"github.com/strangelove-ventures/interchaintest/v4/relayer/rly"
	"github.com/strangelove-ventures/interchaintest/v4/testreporter"
	"github.com/strangelove-ventures/interchaintest/v4/testutil"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"
	"go.uber.org/zap/zaptest"
)

const gaiaNeutronICSPath = "gn-ics-path"
const gaiaNeutronIBCPath = "gn-ibc-path"
const gaiaOsmosisIBCPath = "go-ibc-path"
const neutronOsmosisIBCPath = "no-ibc-path"

// sets up and tests a tokenswap between hub and stargaze facilitated by neutron
func TestTokenSwap(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping in short mode")
	}

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
			GasAdjustment: 1.5,
			GasPrices:     "0.0atom",
			ModifyGenesis: setupGaiaGenesis([]string{
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
						Version:    "v1.0.2",
						UidGid:     "1025:1025",
					},
				},
				Bin:                 "neutrond",
				Bech32Prefix:        "neutron",
				Denom:               "untrn",
				GasPrices:           "0.0untrn,0.0uatom",
				GasAdjustment:       1.3,
				TrustingPeriod:      "1197504s",
				NoHostMount:         false,
				ModifyGenesis:       setupNeutronGenesis("0.05", []string{"untrn"}, []string{"uatom"}),
				ConfigFileOverrides: configFileOverrides,
			},
		},

		{Name: "osmosis", Version: "v11.0.0"},
	})

	chains, err := cf.Chains(t.Name())
	require.NoError(t, err)

	// We have three chains
	atom, neutron, osmosis := chains[0], chains[1], chains[2]
	cosmosAtom, cosmosNeutron, cosmosOsmosis := atom.(*cosmos.CosmosChain), neutron.(*cosmos.CosmosChain), osmosis.(*cosmos.CosmosChain)

	// Relayer Factory
	client, network := ibctest.DockerSetup(t)
	r := ibctest.NewBuiltinRelayerFactory(
		ibc.CosmosRly,
		zaptest.NewLogger(t),
		relayer.CustomDockerImage("ghcr.io/cosmos/relayer", "v2.3.1", rly.RlyDefaultUidGid),
		relayer.RelayerOptionExtraStartFlags{Flags: []string{"-d", "--log-format", "console"}},
	).Build(t, client, network)

	// Prep Interchain
	ic := ibctest.NewInterchain().
		AddChain(cosmosAtom).
		AddChain(cosmosNeutron).
		AddChain(cosmosOsmosis).
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
			Chain2:  cosmosOsmosis,
			Relayer: r,
			Path:    neutronOsmosisIBCPath,
		})

	// Log location
	f, err := ibctest.CreateLogFile(fmt.Sprintf("%d.json", time.Now().Unix()))
	require.NoError(t, err)
	// Reporter/logs
	rep := testreporter.NewReporter(f)
	eRep := rep.RelayerExecReporter(t)

	// Build interchain
	err = ic.Build(ctx, eRep, ibctest.InterchainBuildOptions{
		TestName:          t.Name(),
		Client:            client,
		NetworkID:         network,
		BlockDatabaseFile: ibctest.DefaultBlockDatabaseFilepath(),
		SkipPathCreation:  true,
	})
	require.NoError(t, err, "failed to build interchain")

	err = testutil.WaitForBlocks(ctx, 10, atom, neutron, osmosis)
	require.NoError(t, err, "failed to wait for blocks")

	users := ibctest.GetAndFundTestUsers(t, ctx, "default", int64(500_000_000_000), osmosis)
	osmoUser := users[0]
	osmoUserBalInitial, err := osmosis.GetBalance(ctx, osmoUser.Bech32Address(osmosis.Config().Bech32Prefix), osmosis.Config().Denom)
	require.NoError(t, err)
	require.Equal(t, int64(500_000_000_000), osmoUserBalInitial)

	testCtx := &TestContext{
		OsmoClients:               []*ibc.ClientOutput{},
		GaiaClients:               []*ibc.ClientOutput{},
		NeutronClients:            []*ibc.ClientOutput{},
		OsmoConnections:           []*ibc.ConnectionOutput{},
		GaiaConnections:           []*ibc.ConnectionOutput{},
		NeutronConnections:        []*ibc.ConnectionOutput{},
		NeutronTransferChannelIds: make(map[string]string),
		GaiaTransferChannelIds:    make(map[string]string),
		OsmoTransferChannelIds:    make(map[string]string),
		GaiaIcsChannelIds:         make(map[string]string),
		NeutronIcsChannelIds:      make(map[string]string),
	}

	// generate paths
	generatePath(t, ctx, r, eRep, cosmosAtom.Config().ChainID, cosmosNeutron.Config().ChainID, gaiaNeutronIBCPath)
	generatePath(t, ctx, r, eRep, cosmosAtom.Config().ChainID, cosmosOsmosis.Config().ChainID, gaiaOsmosisIBCPath)
	generatePath(t, ctx, r, eRep, cosmosNeutron.Config().ChainID, cosmosOsmosis.Config().ChainID, neutronOsmosisIBCPath)
	generatePath(t, ctx, r, eRep, cosmosNeutron.Config().ChainID, cosmosAtom.Config().ChainID, gaiaNeutronICSPath)

	// create clients
	generateClient(t, ctx, testCtx, r, eRep, gaiaNeutronICSPath, cosmosAtom, cosmosNeutron)
	neutronClients := testCtx.getChainClients(cosmosNeutron.Config().Name)
	atomClients := testCtx.getChainClients(cosmosAtom.Config().Name)

	err = r.UpdatePath(ctx, eRep, gaiaNeutronICSPath, ibc.PathUpdateOptions{
		SrcClientID: &neutronClients[0].ClientID,
		DstClientID: &atomClients[0].ClientID,
	})
	require.NoError(t, err)

	atomNeutronICSConnectionId, neutronAtomICSConnectionId := generateConnections(t, ctx, testCtx, r, eRep, gaiaNeutronICSPath, cosmosAtom, cosmosNeutron)

	generateICSChannel(t, ctx, r, eRep, gaiaNeutronICSPath, cosmosAtom, cosmosNeutron)

	// create connections and link everything up
	generateClient(t, ctx, testCtx, r, eRep, neutronOsmosisIBCPath, cosmosNeutron, cosmosOsmosis)
	neutronOsmosisIBCConnId, osmosisNeutronIBCConnId := generateConnections(t, ctx, testCtx, r, eRep, neutronOsmosisIBCPath, cosmosNeutron, cosmosOsmosis)
	linkPath(t, ctx, r, eRep, cosmosNeutron, cosmosOsmosis, neutronOsmosisIBCPath)

	generateClient(t, ctx, testCtx, r, eRep, gaiaOsmosisIBCPath, cosmosAtom, cosmosOsmosis)
	gaiaOsmosisIBCConnId, osmosisGaiaIBCConnId := generateConnections(t, ctx, testCtx, r, eRep, gaiaOsmosisIBCPath, cosmosAtom, cosmosOsmosis)
	linkPath(t, ctx, r, eRep, cosmosAtom, cosmosOsmosis, gaiaOsmosisIBCPath)

	generateClient(t, ctx, testCtx, r, eRep, gaiaNeutronIBCPath, cosmosAtom, cosmosNeutron)
	atomNeutronIBCConnId, neutronAtomIBCConnId := generateConnections(t, ctx, testCtx, r, eRep, gaiaNeutronIBCPath, cosmosAtom, cosmosNeutron)
	linkPath(t, ctx, r, eRep, cosmosAtom, cosmosNeutron, gaiaNeutronIBCPath)

	// Start the relayer and clean it up when the test ends.
	require.NoError(t,
		r.StartRelayer(ctx, eRep, gaiaNeutronICSPath, gaiaNeutronIBCPath, gaiaOsmosisIBCPath, neutronOsmosisIBCPath),
		"failed to start relayer with given paths",
	)
	t.Cleanup(func() {
		err = r.StopRelayer(ctx, eRep)
		if err != nil {
			t.Logf("failed to stop relayer: %s", err)
		}
	})

	err = testutil.WaitForBlocks(ctx, 2, atom, neutron, osmosis)
	require.NoError(t, err, "failed to wait for blocks")

	createValidator(t, ctx, r, eRep, atom, neutron)

	// Once the VSC packet has been relayed, x/bank transfers are
	// enabled on Neutron and we can fund its account.
	// The funds for this are sent from a "faucet" account created
	// by interchaintest in the genesis file.
	users = ibctest.GetAndFundTestUsers(t, ctx, "default", int64(500_000_000_000), atom, neutron)
	gaiaUser, neutronUser := users[0], users[1]
	_, _ = gaiaUser, neutronUser

	err = testutil.WaitForBlocks(ctx, 10, atom, neutron, osmosis)
	require.NoError(t, err, "failed to wait for blocks")

	var osmoNeutronChannelId, neutronOsmoChannelId string
	var osmoGaiaChannelId, gaiaOsmoChannelId string
	var neutronGaiaICSChannelId, gaiaNeutronICSChannelId string
	var neutronGaiaTransferChannelId, gaiaNeutronTransferChannelId string

	neutronChannelInfo, _ := r.GetChannels(ctx, eRep, cosmosNeutron.Config().ChainID)
	gaiaChannelInfo, _ := r.GetChannels(ctx, eRep, cosmosAtom.Config().ChainID)
	osmoChannelInfo, _ := r.GetChannels(ctx, eRep, cosmosOsmosis.Config().ChainID)

	// Find all pairwise channels
	osmoNeutronChannelId, neutronOsmoChannelId = getPairwiseTransferChannelIds(testCtx, osmoChannelInfo, neutronChannelInfo, osmosisNeutronIBCConnId, neutronOsmosisIBCConnId, osmosis.Config().Name, neutron.Config().Name)
	osmoGaiaChannelId, gaiaOsmoChannelId = getPairwiseTransferChannelIds(testCtx, osmoChannelInfo, gaiaChannelInfo, osmosisGaiaIBCConnId, gaiaOsmosisIBCConnId, osmosis.Config().Name, cosmosAtom.Config().Name)
	gaiaNeutronTransferChannelId, neutronGaiaTransferChannelId = getPairwiseTransferChannelIds(testCtx, gaiaChannelInfo, neutronChannelInfo, atomNeutronIBCConnId, neutronAtomIBCConnId, cosmosAtom.Config().Name, neutron.Config().Name)
	gaiaNeutronICSChannelId, neutronGaiaICSChannelId = getPairwiseCCVChannelIds(testCtx, gaiaChannelInfo, neutronChannelInfo, atomNeutronICSConnectionId, neutronAtomICSConnectionId, cosmosAtom.Config().Name, cosmosNeutron.Config().Name)

	// Print out connections and channels for debugging
	print("\n osmoGaiaChannelId: ", osmoGaiaChannelId)
	print("\n osmoNeutronChannelId: ", osmoNeutronChannelId)
	print("\n gaiaOsmoChannelId: ", gaiaOsmoChannelId)
	print("\n gaiaNeutronTransferChannelId: ", gaiaNeutronTransferChannelId)
	print("\n gaiaNeutronICSChannelId: ", gaiaNeutronICSChannelId)
	print("\n neutronOsmoChannelId: ", neutronOsmoChannelId)
	print("\n neutronGaiaTransferChannelId: ", neutronGaiaTransferChannelId)
	print("\n neutronGaiaICSChannelId: ", neutronGaiaICSChannelId)

	print("\n osmosisNeutronIBCConnId: ", osmosisNeutronIBCConnId)
	print("\n neutronOsmosisIBCConnId: ", neutronOsmosisIBCConnId)
	print("\n neutronGaiaTransferConnectionId: ", neutronAtomIBCConnId)
	print("\n neutronGaiaICSConnectionId: ", neutronAtomICSConnectionId)
	print("\n gaiaOsmosisIBCConnId: ", gaiaOsmosisIBCConnId)
	print("\n gaiaNeutronTransferConnectionId: ", atomNeutronIBCConnId)
	print("\n gaiaNeutronICSConnectionId: ", atomNeutronICSConnectionId)

	print("\n neutron channels: ")
	for key, value := range testCtx.NeutronTransferChannelIds {
		fmt.Printf("Key: %s, Value: %s\n", key, value)
	}
	print("\n osmo channels: ")
	for key, value := range testCtx.OsmoTransferChannelIds {
		fmt.Printf("Key: %s, Value: %s\n", key, value)
	}
	print("\n gaia channels: ")
	for key, value := range testCtx.GaiaTransferChannelIds {
		fmt.Printf("Key: %s, Value: %s\n", key, value)
	}
	print("\n gaia ics channels: ")
	for key, value := range testCtx.GaiaIcsChannelIds {
		fmt.Printf("Key: %s, Value: %s\n", key, value)
	}
	print("\n neutron ics channels: ")
	for key, value := range testCtx.NeutronIcsChannelIds {
		fmt.Printf("Key: %s, Value: %s\n", key, value)
	}
}
