#!/bin/bash

set -eo pipefail

info() { echo -e "[-] ($(date '+%Y-%m-%d %H:%M:%S')) ${*}"; }
error() { echo -e "[!] ($(date '+%Y-%m-%d %H:%M:%S')) ${*}"; }

cargoclean() {
	# Clean the artifacts of this project ("bin" or "lib"), but not those of the dependencies
	names="$(cargo metadata --format-version 1 | jq -r '.packages[].targets[] | select( .kind | map(. == "bin" or . == "lib") | any ) | select ( .src_path | contains(".cargo/registry") | . != true ) | .name')"
	for name in $names; do
		cargo clean -p "$name" || cargo clean
	done
}

# Get the directory in which this script is contained
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null && pwd )"

# Get the root directory of the crate, which is the first argument or the current folder
CRATE_ROOT="$(cd "${1:-.}" && pwd)"
cd "$CRATE_ROOT"

if [[ ! -r "$CRATE_ROOT/Cargo.toml" ]]; then
	error "Path '$CRATE_ROOT' does not look like the source of a crate"
	exit 1
fi
info "Using CRATE_ROOT=$CRATE_ROOT"

GLOBAL_BLACKLIST="$DIR/../crates/global_blacklist.csv"
if [[ ! -r "$GLOBAL_BLACKLIST" ]]; then
	error "Could not read file '$GLOBAL_BLACKLIST' (global blacklist)"
	exit 1
fi

EVALUATION_TIMEOUT="${EVALUATION_TIMEOUT:-900}"
info "Using EVALUATION_TIMEOUT=$EVALUATION_TIMEOUT seconds"

FORCE_PRUSTI_FILTER="${FORCE_PRUSTI_FILTER:-true}"
info "Using FORCE_PRUSTI_FILTER=$FORCE_PRUSTI_FILTER"

FINE_GRAINED_EVALUATION="${FINE_GRAINED_EVALUATION:-false}"
info "Using FINE_GRAINED_EVALUATION=$FINE_GRAINED_EVALUATION"

BASELINE_EVALUATION="${BASELINE_EVALUATION:-true}"
info "Using BASELINE_EVALUATION=$BASELINE_EVALUATION"

export PRUSTI_CHECK_PANICS="${PRUSTI_CHECK_PANICS:-false}"
info "Using PRUSTI_CHECK_PANICS=$PRUSTI_CHECK_PANICS"

export PRUSTI_CHECK_BINARY_OPERATIONS="${PRUSTI_CHECK_BINARY_OPERATIONS:-false}"
info "Using PRUSTI_CHECK_BINARY_OPERATIONS=$PRUSTI_CHECK_BINARY_OPERATIONS"

export PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT="${PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT:-false}"
info "Using PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT=$PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT"

export RUSTUP_TOOLCHAIN="$(cat "$DIR/../../rust-toolchain")"
info "Using RUSTUP_TOOLCHAIN=$RUSTUP_TOOLCHAIN"

CARGO_PRUSTI="$DIR/../../bin/cargo-prusti"
info "Using CARGO_PRUSTI=$CARGO_PRUSTI"

CARGO_PRUSTI_FILTER="$DIR/../../bin/cargo-prusti-filter"
info "Using CARGO_PRUSTI_FILTER=$CARGO_PRUSTI_FILTER"

# Make sure that the "standard" compilation uses the same compiler flags as Prusti uses
export RUSTFLAGS="-Zborrowck=mir -Zpolonius -Znll-facts"
export POLONIUS_ALGORITHM="Naive"

if [[ "$BASELINE_EVALUATION" != "false" ]] ; then
	info "Run standard compilation"

	exit_status="0"
	cargo clean || exit_status="$?"
	if [[ "$exit_status" != "0" ]]; then
		info "The crate does not compile (cargo clean failed with exit status $exit_status). Skip verification."
		exit 42
	fi
	# Timeout in seconds
	timeout -k 10 $EVALUATION_TIMEOUT cargo build || exit_status="$?"
	if [[ "$exit_status" != "0" ]]; then
		info "The crate does not compile (cargo build failed with exit status $exit_status). Skip verification."
		exit 42
	fi
fi

# Delete old Prusti configurations
rm -f "$CRATE_ROOT/Prusti.toml"

info "Filter supported procedures"

if [[ ! -r "$CRATE_ROOT/prusti-filter-results.json" ]] || [[ "$FORCE_PRUSTI_FILTER" == "true" ]] ; then
	rm -f "$CRATE_ROOT/prusti-filter-results.json"
	export RUST_BACKTRACE=1
	export PRUSTI_REPORT_SUPPORT_STATUS=0
	exit_status="0"
	cargoclean
	# Timeout in seconds
	timeout -k 10 $EVALUATION_TIMEOUT "$CARGO_PRUSTI_FILTER" -j 1 || exit_status="$?"
	unset RUST_BACKTRACE
	unset PRUSTI_REPORT_SUPPORT_STATUS
	if [[ "$exit_status" != "0" ]]; then
		info "The automatic filtering of verifiable functions failed with exit status $exit_status."
		exit 43
	fi
fi

# Collect supported procedures
# `diff` is used to filter out procedures that are in `$GLOBAL_BLACKLIST`
# Source: https://stackoverflow.com/a/18205289/2491528
supported_procedures="$(
    diff --new-line-format="" --unchanged-line-format="" \
        <(jq '.functions[] | select(.procedure.restrictions | length == 0) | .node_path' "$CRATE_ROOT/prusti-filter-results.json" | sort) \
        <(sort "$GLOBAL_BLACKLIST") \
    || true
)"
num_supported_procedures="$(echo "$supported_procedures" | grep . | wc -l || true)"
info "Number of supported procedures in crate: $num_supported_procedures"
#info "Supported procedures in crate:\n$supported_procedures"

if [[ "$BASELINE_EVALUATION" != "false" ]] ; then
	# Clean compilation cache
	cargo clean
fi

# Save disk space
rm -rf log/ nll-facts/
# This is important! Without this, NLL facts are not recomputed and dumped to nll-facts.
rm -rf target/*/incremental/
export PRUSTI_FULL_COMPILATION=true
export RUST_BACKTRACE=1
# Sometimes Prusti is run over dependencies, in a different folder. So, make sure that the whitelist is always enabled.
export PRUSTI_ENABLE_WHITELIST=true

if [[ "$FINE_GRAINED_EVALUATION" == "false" ]] ; then

	info "Prepare whitelist with $num_supported_procedures items"

	(
		echo "CHECK_PANICS = $PRUSTI_CHECK_PANICS"
		echo "CHECK_BINARY_OPERATIONS = $PRUSTI_CHECK_BINARY_OPERATIONS"
		echo "ENCODE_UNSIGNED_NUM_CONSTRAINT = $PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT"
		echo "ENABLE_WHITELIST = true"
		echo "WHITELIST = ["
		echo "$supported_procedures" | sed 's/$/,/' | sed '$ s/.$//'
		echo "]"
	) > "$CRATE_ROOT/Prusti.toml"

	info "Start verification"

	cargo clean
	exit_status="0"
	# Timeout in seconds
	timeout -k 10 $EVALUATION_TIMEOUT "$CARGO_PRUSTI" -j 1 || exit_status="$?"
	if [[ "$exit_status" != "0" ]]; then
		info "Prusti verification failed with exit status $exit_status."
		if [[ "$exit_status" == "124" ]]; then
			exit 124
		else
			exit 101
		fi
	else
		exit 0
	fi

else

	info "Run fine-grained evaluation of $num_supported_procedures items"

	# Hack to set $final_exit_status from the pipe
	FINE_GRAINED_EXIT_STATUS="$CRATE_ROOT/prusti-fine-grained-exit-status.log"
	rm -f "$FINE_GRAINED_EXIT_STATUS"
	touch "$FINE_GRAINED_EXIT_STATUS"

	echo "$supported_procedures" | (grep . || true) | while read procedure_path
	do
		info "Prepare whitelist with just $procedure_path"

		(
			echo "CHECK_PANICS = $PRUSTI_CHECK_PANICS"
			echo "CHECK_BINARY_OPERATIONS = $PRUSTI_CHECK_BINARY_OPERATIONS"
			echo "ENCODE_UNSIGNED_NUM_CONSTRAINT = $PRUSTI_ENCODE_UNSIGNED_NUM_CONSTRAINT"
			echo "ENABLE_WHITELIST = true"
			echo "WHITELIST = ["
			echo "    $procedure_path"
			echo "]"
		) > "$CRATE_ROOT/Prusti.toml"

		info "Start verification of $procedure_path"

		cargoclean
		exit_status="0"
		# Timeout in seconds
		timeout -k 10 $EVALUATION_TIMEOUT "$CARGO_PRUSTI" -j 1 || exit_status="$?"
		if [[ "$exit_status" != "0" ]]; then
			info "Prusti verification failed with exit status $exit_status (item $procedure_path)."
			echo "$exit_status, $procedure_path" >> "$FINE_GRAINED_EXIT_STATUS"
		fi
	done

	final_exit_status="$(echo "$(cat "$FINE_GRAINED_EXIT_STATUS")" | sort -n | tail -n 1 | cut -d ',' -f 1 | sed 's/^$/0/')"
	info "Final exit status: $final_exit_status"
	exit $final_exit_status
fi
