# RRC11 historical baseline audit (2019-08-21)

**Scope:** selected-observer audit of one baseline RIB (`rrc11/bview.20190821.0000.gz`). This is NOT an all-RIS audit.

- Baseline bview timestamp: `2019-08-21T00:00:00Z`
- RIB source SHA-256: `37e0f94d60b4b8bd52a9d66c590994d6b2541ae74ec860bb0ee7f38a8fdcd791`
- Session count (all peers): 39 (24 IPv4, 15 IPv6)
- Total routes in baseline: 7,257,675

## Direct AS11164 / I2PX session (historical evidence)

- Direct session with peer ASN 11164 present in the 2019 bview: **NO**
  - (no peer row with peer ASN 11164 in the baseline peer table)
- Routes received from AS11164: 0
- AS11164 appears inside some other session's AS path: YES (indirect observation, distinct from a direct session)

**The current peer list (RRC11/NYIIX direct peer AS11164) is supporting context only.** It does not establish a 2019 session; the bview peer table above is the evidence.

## AS2603-origin visibility at RRC11

- AS2603-origin route count: 106
- Distinct AS2603 prefixes: 106
- Sessions carrying AS2603-origin routes: 18
- AS2603-origin path distribution: {'per_plane': [('internet2-i2px', 0), ('internet2-re', 0)], 'neither_plane': 106, 'total': 106}

## Qualifying observer-prefix streams (direct I2PX pilot)

- **No qualifying AS2603-origin baseline via the I2PX plane**: no AS2603-origin route at RRC11 contains the I2PX plane ASN in its path, and no direct AS11164 session exists in the baseline. The direct I2PX pilot has **no qualifying baseline** at RRC11; absence of a baseline is NOT evidence of no I2PX-plane event change.

## Session table (all peers in the baseline)

| peer IP | peer ASN | af | total routes | AS2603 routes | AS2603 prefixes |
|---|---|---:|---:|---:|---:|
| 198.32.160.42 | 2497 | ipv4 | 762582 | 12 | 12 |
| 2001:504:1::a500:2497:1 | 2497 | ipv6 | 70709 | 1 | 1 |
| 198.32.160.25 | 2516 | ipv4 | 6050 | 0 | 0 |
| 198.32.160.187 | 6233 | ipv4 | 152645 | 0 | 0 |
| 2001:504:1::a500:6233:1 | 6233 | ipv6 | 71580 | 1 | 1 |
| 198.32.160.61 | 6939 | ipv4 | 131357 | 0 | 0 |
| 2001:504:1::a500:6939:1 | 6939 | ipv6 | 71583 | 1 | 1 |
| 198.32.160.45 | 8966 | ipv4 | 3360 | 0 | 0 |
| 198.32.160.182 | 9002 | ipv4 | 765356 | 12 | 12 |
| 2001:504:1::a500:9002:1 | 9002 | ipv6 | 71184 | 1 | 1 |
| 198.32.160.39 | 9304 | ipv4 | 760312 | 12 | 12 |
| 2001:504:1::a500:9304:1 | 9304 | ipv6 | 73008 | 1 | 1 |
| 198.32.160.121 | 10310 | ipv4 | 195 | 0 | 0 |
| 2001:504:1::a501:310:1 | 10310 | ipv6 | 104 | 0 | 0 |
| 198.32.160.108 | 10848 | ipv4 | 4 | 0 | 0 |
| 2001:504:1::a501:848:1 | 10848 | ipv6 | 1 | 0 | 0 |
| 198.32.160.87 | 11403 | ipv4 | 79 | 0 | 0 |
| 198.32.160.103 | 13030 | ipv4 | 730824 | 12 | 12 |
| 2001:504:1::a501:3030:1 | 13030 | ipv6 | 58581 | 1 | 1 |
| 198.32.160.113 | 15547 | ipv4 | 773333 | 12 | 12 |
| 2001:504:1::a501:5547:1 | 15547 | ipv6 | 73329 | 1 | 1 |
| 198.32.160.175 | 15695 | ipv4 | 17 | 0 | 0 |
| 2001:504:1::a501:5695:1 | 15695 | ipv6 | 2 | 0 | 0 |
| 198.32.160.40 | 16570 | ipv4 | 11 | 0 | 0 |
| 198.32.160.137 | 19151 | ipv4 | 761674 | 12 | 12 |
| 2001:504:1::a501:9151:1 | 19151 | ipv6 | 74491 | 1 | 1 |
| 198.32.160.47 | 20940 | ipv4 | 27 | 0 | 0 |
| 2001:504:1::a502:940:1 | 20940 | ipv6 | 5 | 0 | 0 |
| 198.32.160.15 | 22691 | ipv4 | 73 | 0 | 0 |
| 198.32.160.60 | 22691 | ipv4 | 74 | 0 | 0 |
| 198.32.160.242 | 24482 | ipv4 | 767029 | 12 | 12 |
| 2001:504:1::a502:4482:1 | 24482 | ipv6 | 73279 | 1 | 1 |
| 198.32.160.100 | 27257 | ipv4 | 126 | 0 | 0 |
| 198.32.160.107 | 29838 | ipv4 | 139 | 0 | 0 |
| 198.32.160.124 | 46450 | ipv4 | 99 | 0 | 0 |
| 2001:504:1::a504:6450:1 | 46450 | ipv6 | 18 | 0 | 0 |
| 198.32.160.168 | 51185 | ipv4 | 770009 | 12 | 12 |
| 198.32.160.58 | 397143 | ipv4 | 162825 | 0 | 0 |
| 2001:504:1::a539:7143:1 | 397143 | ipv6 | 71601 | 1 | 1 |
