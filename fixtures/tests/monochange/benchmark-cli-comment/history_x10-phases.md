#### Phase timings

##### `mc release --dry-run`

| Phase                        | Budget [ms] | main [ms] | pr [ms] | Δ pr-main [ms] | Status    |
| :--------------------------- | ----------: | --------: | ------: | -------------: | :-------- |
| `prepare release total`      |        1600 |       972 |    1044 |            +72 | regressed |
| `discover release workspace` |         700 |       388 |     412 |            +24 | regressed |
| `parse changeset files`      |         320 |       186 |     201 |            +15 | regressed |
| `read changeset files`       |         320 |       149 |     160 |            +11 | regressed |
| `build manifest updates`     |         180 |        81 |      87 |             +6 | regressed |

##### `mc release`

| Phase                        | Budget [ms] | main [ms] | pr [ms] | Δ pr-main [ms] | Status    |
| :--------------------------- | ----------: | --------: | ------: | -------------: | :-------- |
| `prepare release total`      |        1800 |      1182 |    1261 |            +79 | regressed |
| `discover release workspace` |         700 |       388 |     414 |            +26 | regressed |
| `parse changeset files`      |         320 |       186 |     201 |            +15 | regressed |
| `apply release changes`      |         260 |       132 |     146 |            +14 | regressed |
| `build manifest updates`     |         180 |        81 |      88 |             +7 | regressed |
