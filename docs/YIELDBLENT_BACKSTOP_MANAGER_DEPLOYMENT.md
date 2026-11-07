# YieldBlent Backstop Manager Deployment

This runbook prepares and verifies the dedicated `backstop-manager` instance
for the YieldBlent pool on Stellar mainnet. It does not transfer tokens or
deposit into the backstop. The instance delegates high-scope management to the
YieldBlox DAO while retaining a separately supplied owner that can replace the
manager, change the allowlists, recover held tokens, or begin the contract's
two-step ownership transfer.

## Reviewed bindings

- Manager: YieldBlox DAO
  `CANSYFVMIP7JVYEZQ463Y2I2VLEVNLDJJ4QNZTDBGLOOGKURPTW4A6FQ`
- Manager scope: `2` (`High`)
- Allowed backstop: BLNT V3 backstop
  `CAHCUKBPTW5DR5RNXOHCTJD5U76A6EEZXYZAQFRQEEQRXQ3CRUZ6Z3MO`
- Allowed pool: YieldBlent
  `CAUTPSQDAJEUV5XEU3EXFVYRGDJLEJGVTFJCFZCSPTDCREAMZ573VF4D`
- Initial manager harvest share: `0` basis points
- Source metadata: `source_repo=github:BLNT-Protocol/backstop-manager`

The owner is deliberately not hardcoded or committed. It is a required
constructor input supplied through `BLNT_BACKSTOP_MANAGER_OWNER`. The deployer
is independently supplied through `BLNT_BACKSTOP_MANAGER_DEPLOYER`, so the
deployment account and the long-lived owner need not be the same address.

## Build and plan

Initialize all submodules and build the exact optimized artifact:

```bash
git submodule update --init --recursive
make -C backstop-manager test
make -C backstop-manager build
```

Set the public deployment and owner addresses, then print the complete plan and
predicted contract ID:

```bash
export BLNT_BACKSTOP_MANAGER_DEPLOYER="G..."
export BLNT_BACKSTOP_MANAGER_OWNER="G..."
make mainnet-backstop-manager-plan
```

The plan rebuilds the artifact, verifies its `source_repo` metadata, validates
all constructor addresses, fixes the manager scope at `2`, and derives the
contract ID from the deployer plus the committed deployment salt. It performs
no network mutation.

## Deploy

The account config is a local, uncommitted shell file containing exactly the
deployer credentials used by Stellar CLI:

```bash
STELLAR_PUBLIC_KEY="G..."
STELLAR_SECRET_KEY="S..."
```

After committing, pushing, and reviewing both the `backstop-manager` source
commit and the parent submodule pin, run:

```bash
export BLNT_BACKSTOP_MANAGER_DEPLOYER="G..."
export BLNT_BACKSTOP_MANAGER_OWNER="G..."
export BLNT_BACKSTOP_MANAGER_ACCOUNT_CONFIG="/absolute/path/to/mainnet.conf"
export BLNT_BACKSTOP_MANAGER_CONFIRM_MAINNET=true
make mainnet-backstop-manager-deploy
```

Deployment fails closed unless the parent repository and submodule are clean,
the parent commit pins the checked-out source commit, the credential file
matches the declared deployer, and the explicit mainnet confirmation is set.
It uploads the reviewed WASM without re-optimizing it, deploys the predicted
contract with the reviewed bindings, reads all constructor state back from
mainnet, and byte-compares the deployed WASM with the local artifact.

Local transaction evidence and cost logs are written under the ignored
`.mainnet-v3-backstop-manager/` directory. After successful verification, the
script writes the public, secret-free deployment record to
`deployments/mainnet-v3-yieldblent-backstop-manager.json`. That record is not
created before a real deployment. If submission succeeds but the CLI exits
before local state is updated, a rerun recovers the signed transaction hash
from those logs and verifies its successful result and timestamp through
Horizon. The registry is not written without that deployment evidence.

## Read-only verification

With either the local state file or the committed deployment record present:

```bash
make mainnet-backstop-manager-status
```

The status command verifies:

- the deterministic contract ID;
- byte-for-byte deployed WASM equality;
- the canonical source-repository metadata;
- the owner and YieldBlox DAO manager;
- high manager scope;
- the single allowed BLNT V3 backstop and YieldBlent pool; and
- an initial `harvest_bps` value of zero.

## Post-deployment governance check

The YieldBlox DAO address is a Governor contract. Its successful constructor
binding proves only that it is the configured manager; it does not prove that
the DAO proposal flow can execute a manager action. Before funding this
instance, execute a DAO proposal against a harmless or intentionally bounded
manager call and verify the resulting authorization path. Keep token funding
and the lockup-backfill integration out of the deployment transaction so each
step has independent evidence and can be stopped safely.
