#!/usr/bin/env bash
# Host gate (N1) — everything that must pass before a commit.
#
#   ./scripts/check.sh          # cargo test workspace + registry lint
#   ./scripts/check.sh lint     # registry lint only (skip cargo test)
#
# Layer 2 runs `aginx commands --check` over a scratch copy of
# rootfs/usr/bin faces (scripts + .aginxmd; ELF stubs for compiled faces).
# Layer 1 is the crate unit tests.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT}"

MODE="${1:-all}"

# ---- 1. host tests ---------------------------------------------------------
# aginx-svc's supervisor bin (aginx-svcd) is Linux-only (prctl/ucred/SO_PEERCRED),
# same discipline as agupd/agsvc in the first-gen repo: on macOS the bin is
# excluded and only its lib (host-testable, also what aginx-term links) runs.
if [ "${MODE}" != "lint" ]; then
  if [ "$(uname -s)" = "Linux" ]; then
    echo "==> cargo test --workspace"
    cargo test --workspace
  else
    echo "==> cargo test --workspace --exclude aginx-svc (+ its lib)"
    cargo test --workspace --exclude aginx-svc
    cargo test -p aginx-svc --lib
  fi
fi

# ---- 2. campix host tests (M47②) --------------------------------------------
# cam-shot's pixel-chain math (black level / gamma LUTs, crop geometry,
# debayer-rotate-scale) lives in each device's cam/campix.h as pure
# functions so it is testable without a device — same zig that builds the
# device binary. D14: one campix_test.c per devices/<codename>/cam/ — every
# machine's math runs, not just the first target's.
if [ "${MODE}" != "lint" ]; then
  ZIG="$(command -v zig || true)"
  test -z "${ZIG}" && ZIG=/opt/homebrew/bin/zig
  test -x "${ZIG}" || { echo "zig not found (needed for campix_test)" >&2; exit 1; }
  found=0
  for ct in "${ROOT}"/devices/*/cam/campix_test.c; do
    test -f "${ct}" || continue
    found=$((found + 1))
    dev="$(basename "$(dirname "$(dirname "${ct}")")")"
    echo "==> campix_test (${dev})"
    CBIN="$(mktemp -d)/campix_test"
    "${ZIG}" cc -O1 -Wall -Wextra -o "${CBIN}" "${ct}" -lm
    "${CBIN}"
    rm -f "${CBIN}"
  done
  test "${found}" -gt 0 || { echo "no devices/*/cam/campix_test.c found" >&2; exit 1; }
fi

# ---- 3. D14 grep gate -------------------------------------------------------
# D14 law 1: no machine strings in platform crates. Every hit must carry
# an inline `// D14-exempt` marker ON THE SAME LINE (a marker on the line
# above does not count — per-line filter), each reviewed by hand:
#   - hwd/img test fixtures asserting the REAL committed device.toml
#     (schema-truth tests) and their helper fns
#   - provenance comments in svc/boot_ok.rs pointing at [slots]/[update]
#     registrations in devices/redfin/device.toml
#   - provenance comments / relay-id law tests in pair (#450) and voice
#     history notes (#467 widening: enchilada/panther + their SoCs)
#   - first-gen frozen-offset exemptions noted in ARCH.md D14
if [ "${MODE}" != "lint" ]; then
  BAD="$(grep -rnE '1080|2340|event[0-9]|qpnp_pon|sm7250|redfin|enchilada|panther|sm845|sdm845|rk3566' \
      "${ROOT}"/crates/*/src --include='*.rs' | grep -v 'D14-exempt' || true)"
  if [ -n "${BAD}" ]; then
    echo "D14 gate: machine strings in platform crates (mark reviewed lines" >&2
    echo "with an inline '// D14-exempt', or move the data to devices/):" >&2
    echo "${BAD}" >&2
    exit 1
  fi
fi


# ---- 3b. HOME Seed→Finalize→Resync gate (omacom borrow, 2026-10-31) --------
# Hard table: Seed=bake home/ only; Finalize≠silent /home write from
# provision; Resync=/home in state tar (preserve), not skel-clobber on boot.
echo "==> HOME-PHASE gate (Seed/Finalize/Resync)"
PROV="${ROOT}/rootfs/etc/init.d/provision"
BRF="${ROOT}/scripts/build-rootfs.sh"
UPD="${ROOT}/crates/update/src/main.rs"
test -f "${PROV}" && test -f "${BRF}" && test -f "${UPD}" \
  || { echo "HOME-PHASE: missing provision/build-rootfs/update" >&2; exit 1; }
for anchor in "HOME-PHASE Seed" "HOME-PHASE Finalize" "HOME-PHASE Resync"; do
  grep -q "${anchor}" "${PROV}" \
    || { echo "HOME-PHASE: provision missing anchor '${anchor}'" >&2; exit 1; }
done
grep -q 'HOME-PHASE Seed' "${BRF}" \
  || { echo "HOME-PHASE: build-rootfs missing Seed anchor" >&2; exit 1; }
grep -q 'cp -R "${ROOT}/home/." "${TREE}/home/"' "${BRF}" \
  || { echo "HOME-PHASE: build-rootfs missing home seed cp" >&2; exit 1; }
grep -q 'OTA 不覆盖 /home' "${BRF}" \
  || { echo "HOME-PHASE: build-rootfs missing OTA-must-not-clobber-/home note" >&2; exit 1; }
BAD_HOME="$(grep -n '/home' "${PROV}" | grep -v 'HOME-PHASE' | grep -vE '^[[:digit:]]+:[[:space:]]*#' || true)"
if [ -n "${BAD_HOME}" ]; then
  echo "HOME-PHASE Finalize: provision must not reference /home:" >&2
  echo "${BAD_HOME}" >&2
  exit 1
fi
for ph in seed repair resync finalize; do
  grep -qE "^# ---- ${ph}" "${PROV}" \
    || { echo "HOME-PHASE: provision missing boot phase '# ---- ${ph}'" >&2; exit 1; }
done
grep -q '/home' "${UPD}" \
  || { echo "HOME-PHASE Resync: update must mention /home in state tar members" >&2; exit 1; }
if grep -R -E 'reinstall-configs|cp -a[f]?[[:space:]].*/etc/skel' \
    "${ROOT}/rootfs/etc/init.d" 2>/dev/null | grep -q .; then
  echo "HOME-PHASE: refusing silent home skel-clobber under rootfs/etc/init.d" >&2
  exit 1
fi

# ---- 3c. PLATFORM-ROOT gate (omacom borrow / D14, 2026-10-31) --------------
# Platform root = machine/OS contract files. Runtime: /etc/aginx/.
# Bake source: devices/<codename>/. Personality = /home only (not here).
echo "==> PLATFORM-ROOT gate (devices → /etc/aginx vs /home)"
HWD="${ROOT}/crates/hwd/src/lib.rs"
BRF="${ROOT}/scripts/build-rootfs.sh"
test -f "${HWD}" && test -f "${BRF}" \
  || { echo "PLATFORM-ROOT: missing hwd or build-rootfs" >&2; exit 1; }
grep -q 'PLATFORM-ROOT' "${BRF}" \
  || { echo "PLATFORM-ROOT: build-rootfs missing PLATFORM-ROOT anchor" >&2; exit 1; }
grep -q 'install -m 644 "${DEVDIR}/device.toml" "${TREE}/etc/aginx/device.toml"' "${BRF}" \
  || { echo "PLATFORM-ROOT: bake must install device.toml → /etc/aginx/device.toml" >&2; exit 1; }
grep -q 'DEVICE_TOML_PATH.*/etc/aginx/device.toml' "${HWD}" \
  || { echo "PLATFORM-ROOT: hwd must read /etc/aginx/device.toml" >&2; exit 1; }

# Every devices/<codename>/ must carry device.toml (file-is-contract).
dev_n=0
for d in "${ROOT}"/devices/*/device.toml; do
  test -f "${d}" || continue
  dev_n=$((dev_n + 1))
  code="$(basename "$(dirname "${d}")")"
  # [device] name should match directory (soft check via grep name)
  grep -q '^\[device\]' "${d}" \
    || { echo "PLATFORM-ROOT: ${code}/device.toml missing [device]" >&2; exit 1; }
done
test "${dev_n}" -ge 1 \
  || { echo "PLATFORM-ROOT: no devices/*/device.toml" >&2; exit 1; }

# Personality must not live under platform recipe /etc/aginx.
if [ -e "${ROOT}/rootfs/etc/aginx/AGENTS.md" ] || [ -e "${ROOT}/rootfs/etc/aginx/MEMORY.md" ] \
   || [ -d "${ROOT}/rootfs/etc/aginx/knowledge" ]; then
  echo "PLATFORM-ROOT: refuse personality under rootfs/etc/aginx (belongs in /home)" >&2
  exit 1
fi
# Machine archive must not live under home seed.
HOME_SEED="${ROOT}/home"
if [ -f "${HOME_SEED}/device.toml" ] || [ -f "${HOME_SEED}/etc/aginx/device.toml" ]; then
  echo "PLATFORM-ROOT: refuse device.toml under home/ seed (belongs in devices/ → /etc/aginx)" >&2
  exit 1
fi
# Recipe etc/aginx is the platform-root shape (groups/secret/svc/env); device.toml is bake-injected.
test -d "${ROOT}/rootfs/etc/aginx" \
  || { echo "PLATFORM-ROOT: missing rootfs/etc/aginx recipe dir" >&2; exit 1; }

# ---- 4. registry lint (CLI file-is-registry + aginx: metadata) -------------
# Faces live in rootfs/usr/bin (scripts + .aginxmd sidecars). Host gate
# plants non-shebang stubs beside sidecars so read_strict uses metadata
# the same way on-device ELF + .aginxmd does. (Repo once had shims/; that
# tree is gone — do not resurrect an empty parallel registry.)
cargo build -p aginx-router --release >/dev/null
SCRATCH="$(mktemp -d)"
trap 'rm -rf "${SCRATCH}"' EXIT

mkdir -p "${SCRATCH}/usr/bin" "${SCRATCH}/home"
cp -R "${ROOT}/rootfs/usr/bin/." "${SCRATCH}/usr/bin/"
for md in "${SCRATCH}"/usr/bin/*.aginxmd; do
  [ -f "${md}" ] || continue
  base="${md%.aginxmd}"
  if [ ! -e "${base}" ]; then
    printf 'ELFSTUB' > "${base}"
    chmod 755 "${base}"
  fi
done
chmod 755 "${SCRATCH}"/usr/bin/aginx-* 2>/dev/null || true

echo "==> CLI-ROUTER gate (file route + aginx: metadata)"
# Contract anchors (omacom borrow): router is file-is-registry; --check is lint.
grep -q 'file-is-registry' "${ROOT}/crates/router/src/main.rs" \
  || { echo "CLI-ROUTER: main.rs missing file-is-registry contract note" >&2; exit 1; }
grep -q 'aginx:summary' "${ROOT}/crates/router/src/meta.rs" \
  || { echo "CLI-ROUTER: meta.rs missing summary key" >&2; exit 1; }
grep -q '"--check"' "${ROOT}/crates/router/src/main.rs" \
  || { echo "CLI-ROUTER: commands --check missing" >&2; exit 1; }

echo "==> aginx commands --check (rootfs/usr/bin faces)"
AGINX_CMD_PATH="${SCRATCH}/usr/bin" \
AGINX_HOME="${SCRATCH}/home" \
AGINX_GROUPS_DESC="${ROOT}/rootfs/etc/aginx/groups.desc" \
  "${ROOT}/target/release/aginx" commands --check \
  || { echo "aginx commands --check failed — fix rootfs/usr/bin faces" >&2; exit 1; }

# Plumbing must stay callable but off the default menu (omacom hidden pattern).
export AGINX_CMD_PATH="${SCRATCH}/usr/bin"
export AGINX_HOME="${SCRATCH}/home"
export AGINX_GROUPS_DESC="${ROOT}/rootfs/etc/aginx/groups.desc"
MENU="$("${ROOT}/target/release/aginx" commands)"
for plumbing in boot-ok done download log-trim panel-off; do
  echo "${MENU}" | grep -E "^  ${plumbing}[[:space:]]" >/dev/null \
    && { echo "CLI-ROUTER: plumbing '${plumbing}' must be aginx:hidden=true (still in menu)" >&2; exit 1; }
done

echo "check: all green"
