# Cross-observer matrix — NORDUnet pilot (Session 35, Part 6)

**Reviewed target:** NORDUnet (AS2603) · **window:** 2019-08-21 16:00:00Z – 17:30:00Z

Each row is an **independent AnalysisRun**; observations are never merged into one
verdict. **Direct** (peer ASN equals the plane ASN) and **indirect** (path contains
the plane ASN) are distinct evidence classes. Collector location describes where the
collector is hosted, not the path taken by observed routes.

## R&E-plane runs (cohort selector `ContainsAny[11537]`)

| collector | location | peer sessions | streams | prefixes | absences | replacements | R&E departures | R&E returns | restoration | verdict |
|---|---|---|---:|---:|---:|---:|---:|---:|---|---|
| route-views2 | Eugene, Oregon, US | 137.164.16.84 (AS2152, indirect R&E (path contains AS11537)); 203.181.248.168 (AS7660, indirect R&E (path contains AS11537)); 64.57.28.241 (AS11537, direct R&E (peer ASN 11537)) | 33 | 11 | 11 | 30 | 22 | 33 | 11 of 11 absent streams restored | ExpectedLossOfReachability |
| rrc00 | Amsterdam, Netherlands | 203.119.104.1 (AS4608, indirect R&E (path contains AS11537)) | 11 | 11 | 0 | 0 | 0 | 0 | 0 of 0 absent streams restored | LessImpactThanExpected |
| rrc06 | Otemachi, Tokyo, Japan | 2001:200:0:fe00::12a9:0 (AS4777, indirect R&E (path contains AS11537)); 202.249.2.20 (AS4777, indirect R&E (path contains AS11537)) | 12 | 12 | 0 | 14 | 12 | 12 | 0 of 0 absent streams restored | LessImpactThanExpected |
| rrc15 | Sao Paulo, Brazil | 187.16.216.4 (AS1916, indirect R&E (path contains AS11537)); 187.16.218.21 (AS52888, indirect R&E (path contains AS11537)); 2001:12f8::20 (AS28571, indirect R&E (path contains AS11537)); 2001:12f8::218:21 (AS52888, indirect R&E (path contains AS11537)) | 24 | 12 | 0 | 37 | 13 | 13 | 0 of 0 absent streams restored | LessImpactThanExpected |

### Evidence intervals

- **route-views2**: 2019-08-21T16:45:25+00:00 .. 2019-08-21T17:02:19+00:00
- **rrc00**: None .. None
- **rrc06**: 2019-08-21T16:45:44+00:00 .. 2019-08-21T17:02:38+00:00
- **rrc15**: 2019-08-21T16:35:38+00:00 .. 2019-08-21T17:52:16+00:00

## I2PX-plane preflights (cohort selector `ContainsAny[11164]`)

| collector | qualifying frozen streams | outcome |
|---|---:|---|
| route-views2 | 0 | no I2PX-plane baseline at this observer (no AS11164 session or AS11164-in-path route in the 2019-08-21 baseline); absence of a baseline is NOT evidence of no I2PX-plane event change |
| rrc00 | 0 | no I2PX-plane baseline at this observer (no AS11164 session or AS11164-in-path route in the 2019-08-21 baseline); absence of a baseline is NOT evidence of no I2PX-plane event change |
| rrc06 | 0 | no I2PX-plane baseline at this observer (no AS11164 session or AS11164-in-path route in the 2019-08-21 baseline); absence of a baseline is NOT evidence of no I2PX-plane event change |
| rrc15 | 0 | no I2PX-plane baseline at this observer (no AS11164 session or AS11164-in-path route in the 2019-08-21 baseline); absence of a baseline is NOT evidence of no I2PX-plane event change |

