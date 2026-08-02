# External URL status — repository truth audit (2026-08-02)

Status of every external URL referenced in current repository
documentation, checked 2026-08-02 with `curl -L` (HTTP status of the
final response). External sites are transient; this record is a dated
snapshot, not a CI gate. Internal links and repository-local paths are
enforced by the documentation drift check instead.

| URL | Status | Notes |
|---|---|---|
| `https://docs.globalnoc.iu.edu/uploads/c5/88/c5881bec35cb83807dd4b0a7ee32effe/MANLAN-20190821-Postmortem.pdf` | checked (200) | MAN LAN AAR source document |
| `https://internet2.edu/community/about-us/policies/privacy/` | checked (200) | Internet2 privacy statement |
| `https://internet2.edu/community/about-us/policies/terms-of-use/` | checked (200) | Internet2 terms of use |
| `https://sn-tools.grnoc.iu.edu/` | checked (200, redirects to `/home/`) | secondary GRNOC public-task-viewer entry point |
| `https://ticket-viewer.grnoc.iu.edu/` | checked (200) | GRNOC Public Task Viewer |
| `https://spaces.bgpkit.org/parser/update-example.gz` | checked (200) | bgpkit-parser fixture source |
| `https://data.ris.ripe.net/` | redirected (→ `https://ris.ripe.net/docs/mrt/`) | RIPE RIS archive root; the redirect target is documented as intentionally historical/stable |
| `http://archive.routeviews.org/` | checked (200) | RouteViews archive root |

`http://127.0.0.1:8080` appears in the README as the loopback workbench
address — it is the local server, not an external resource.
