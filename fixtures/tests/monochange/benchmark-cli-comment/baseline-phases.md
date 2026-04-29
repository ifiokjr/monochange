#### Phase timings

##### `mc release --dry-run`

| Phase                        | Budget [ms] | main [ms] | pr [ms] | Δ pr-main [ms] | Status    |
| :--------------------------- | ----------: | --------: | ------: | -------------: | :-------- |
| `prepare release total`      |         450 |       238 |     251 |            +13 | regressed |
| `discover release workspace` |         180 |        96 |     101 |             +5 | regressed |
| `parse changeset files`      |         120 |        44 |      47 |             +3 | regressed |
| `read changeset files`       |         120 |        31 |      33 |             +2 | regressed |
| `build manifest updates`     |          80 |        18 |      19 |             +1 | regressed |

##### `mc release`

| Phase                        | Budget [ms] | main [ms] | pr [ms] | Δ pr-main [ms] | Status    |
| :--------------------------- | ----------: | --------: | ------: | -------------: | :-------- |
| `prepare release total`      |         550 |       312 |     333 |            +21 | regressed |
| `discover release workspace` |         180 |        96 |     102 |             +6 | regressed |
| `parse changeset files`      |         120 |        44 |      47 |             +3 | regressed |
| `apply release changes`      |         160 |        58 |      63 |             +5 | regressed |
| `build manifest updates`     |          80 |        18 |      20 |             +2 | regressed |
