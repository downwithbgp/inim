# RRC11 direct I2PX pilot decision (2019-08-21)

- Reviewed target: NORDUnet (AS2603)
- Observer relationship: direct AS11164/I2PX session at RRC11 (NYIIX, New York)
- Pilot window: 2019-08-21T16:00:00Z .. 2019-08-21T17:30:00Z
- Baseline bview: bview.20190821.0000.gz (2019-08-21T00:00:00Z)

## Decision: **blocked-no-direct-session**

**Blocking reason:** No direct AS11164/I2PX session exists in the historical RRC11 baseline (bview.20190821.0000.gz, 2019-08-21T00:00:00Z): zero of 39 peer rows carry peer ASN 11164. The current peer list (RRC11/NYIIX direct peer AS11164) is supporting context only and does not establish a 2019 session. The direct I2PX pilot was not executed.

The direct I2PX pilot is not merged with the R&E-plane runs. The target is never broadened to create a result.
