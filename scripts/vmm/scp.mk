.PHONY: vmm_scp

PORT ?= 6657

vmm_scp:
	scp -P $(PORT) $(OUT_BIN) ubuntu@localhost:/home/ubuntu/arceos-intel.bin
