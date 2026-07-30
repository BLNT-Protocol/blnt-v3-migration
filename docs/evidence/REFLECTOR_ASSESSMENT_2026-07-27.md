# Reflector Compatibility Assessment — 2026-07-27

Status: Historical deployment evidence

This records the point-in-time Reflector checks referenced by
[Production Oracle Qualification](../ORACLE_DEPLOYMENT.md). It is not an audit,
current provider endorsement, or production approval.

The contract addresses were taken from
[Stellar's oracle-provider documentation](https://developers.stellar.org/docs/data/oracles/oracle-providers).

| Property | Testnet mainnet-DEX mirror | Mainnet Stellar-DEX feed |
| --- | --- | --- |
| Contract | `CAVLP5DH2GJPZMVO7IJY4CVOD5MWEFTJFVPD2YY2FQXOQHRGHK4D6HLP` | `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M` |
| Version | 6 | 6 |
| Decimals | 14 | 14 |
| Resolution | 300 seconds | 300 seconds |
| Base | projected USDC SAC | mainnet USDC SAC |
| Advertised assets | 9 | 49 |
| Exact BLND | missing | missing |
| Exact native XLM | missing | present |
| BLND history | unavailable | unavailable |
| `prices(XLM, 7)` | unavailable | returned 8 observations |
| WASM SHA-256 | `8ecd1857496df2c15aaab4d18d2d7689542a62814245e9b2c613c609b86bd11c` | `7944a96d512f6e51532687811cc0c932505b0c051be9d351f5332739d70f753c` |
| Admin | `GCLA2E3LQDPAPJLHYDMB5R65ASGLNXWGJCX4TX7XA75C7VTJ7Y2OTZXA` | `GC2M2UTZ57GSENZBTPMLPB6QSAL2USIPHMXSUCT2I3V24GYWF3EI6RFA` |
| Candidate verdict | incompatible | incompatible |

The backstop-valuation contract requires an exact history length. The observed
eight-element response to `prices(XLM, 7)` therefore failed even though its
timestamps were otherwise uniform and fresh. Silently discarding an extra
observation would make the configured averaging window ambiguous.

Both checked contracts exposed an administrator. Selecting either would trust
the provider's publisher consensus, administrator and upgrade process,
availability, and market methodology. The valuation contract rechecks base,
decimals, and resolution on each quote, but cannot prove that an external
contract retained previously reviewed WASM.

No provider was approved and no production valuation contract was deployed by
this assessment. A qualifying feed must provide the exact USDC base, BLND and
native-XLM membership, and configured history semantics.
