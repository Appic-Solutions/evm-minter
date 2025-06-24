# Makefile

# Load environment variables from .env
include .env
export $(shell sed 's/=.*//' .env) # Export all variables from .env



# deploy to bnb testnet
deploy_bsc_testnet:
	@echo "Deploying to BNB testnet"
	@echo "BSCTEST_RPC_URL=$(BSCTEST_RPC_URL)"
	@echo "BNBSCAN_API_KEY"=$(BNBSCAN_API_KEY)
	forge script --chain 97 script/Depositbnbtest.s.sol:DeployDeposit --rpc-url $(BSCTEST_RPC_URL) --broadcast --verifier-url https://api-testnet.bscscan.com/api --etherscan-api-key  $(BNBSCAN_API_KEY)  --verify -vvvv 


# deploy to bnb testnet
deploy_bsc:
	@echo "Deploying to Binance smart chain"
	@echo "BSC_RPC_URL=$(BSC_RPC_URL)"
	@echo "BNBSCAN_API_KEY"=$(BNBSCAN_API_KEY)
	forge script --chain 56 script/Depositbnb.s.sol:DeployDeposit --rpc-url $(BSC_RPC_URL) --broadcast --verifier-url https://api.bscscan.com/api --etherscan-api-key  $(BNBSCAN_API_KEY)  --verify -vvvv 


# deploy to bnb testnet
deploy_base:
	@echo "Deploying to Base"
	@echo "BASE_RPC_URL=$(BASE_RPC_URL)"
	@echo "BASESCAN_API_KEY"=$(BASESCAN_API_KEY)
	forge script --chain 8453 DeployBaseBridge --rpc-url $(BASE_RPC_URL) --broadcast --verifier-url https://api.basescan.org/api --etherscan-api-key  $(BASESCAN_API_KEY)  --verify -vvvv 






