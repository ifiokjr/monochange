| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `main · mc validate` | 120.0 ± 6.0 | 113.0 | 129.0 | 1.00 |
| `pr · mc validate` | 126.0 ± 4.0 | 121.0 | 131.0 | 1.05 ± 0.06 |
| `main · mc discover --format json` | 88.0 ± 3.0 | 84.0 | 92.0 | 1.00 |
| `pr · mc discover --format json` | 85.0 ± 2.0 | 82.0 | 88.0 | 0.97 ± 0.04 |
| `main · mc release --dry-run` | 240.0 ± 8.0 | 229.0 | 251.0 | 1.00 |
| `pr · mc release --dry-run` | 255.0 ± 10.0 | 242.0 | 269.0 | 1.06 ± 0.06 |
