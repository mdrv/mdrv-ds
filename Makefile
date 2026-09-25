.PHONY: release check

release:
	cargo build --release -p mdrv-ds-linux
	sudo setcap cap_net_bind_service,cap_net_raw,cap_sys_ptrace+ep target/release/mdrv-ds
	@echo "build + setcap done"

check:
	cargo check --workspace
