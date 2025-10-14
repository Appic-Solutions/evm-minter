# Makefile

# Load environment variables from .env
include .env
export $(shell sed 's/=.*//' .env) # Export all variables from .env




# deploy to bnb 
deploy_bsc:
	@echo "Deploying to Binance smart chain"
	@echo "BSC_RPC_URL=$(BSC_RPC_URL)"
	@echo "ETHSCAN_API_KEY"=$(ETHSCAN_API_KEY)
	forge script --chain 56 DeployBscBridge --rpc-url $(BSC_RPC_URL) --broadcast --verifier-url https://api.bscscan.com/api --etherscan-api-key  $(ETHSCAN_API_KEY)  --verify -vvvv 


# deploy to bnb 
deploy_base:
	@echo "Deploying to Base"
	@echo "BASE_RPC_URL=$(BASE_RPC_URL)"
	@echo "ETHSCAN_API_KEY"=$(BASESCAN_API_KEY)
	forge script --chain 8453 DeployBaseBridge --rpc-url $(BASE_RPC_URL) --broadcast --verifier-url https://api.basescan.org/api --etherscan-api-key  $(ETHSCAN_API_KEY)  --verify -vvvv 


# deploy to pol 
deploy_pol:
	@echo "Deploying to Polygon"
	@echo "POL_RPC_URL=$(POL_RPC_URL)"
	@echo "ETHSCAN_API_KEY"=$(ETHSCAN_API_KEY)
	forge script --chain 137 DeployPolBridge --rpc-url $(POL_RPC_URL) --broadcast --etherscan-api-key $(ETHSCAN_API_KEY)  --verify -vvvv 



# deploy to bnb 
deploy_eth:
	@echo "Deploying to Polygon"
	@echo "ETH_RPC_URL=$(ETH_RPC_URL)"
	@echo "ETHSCAN_API_KEY"=$(ETHSCAN_API_KEY)
	forge script --chain 1 DeployEthBridge --rpc-url $(ETH_RPC_URL) --broadcast --etherscan-api-key $(ETHSCAN_API_KEY)  --verify -vvvv 








