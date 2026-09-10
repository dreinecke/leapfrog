# Lock-screen fingerprint guard

Omarchy 4.0.2 and Lock Screen Explorer 1.5.5 retry fingerprint PAM after
250 ms with no cap. If T1Bridge's keybag relay is unavailable, PAM fails
immediately and the lock screen can create several new attempts per second.

`install` patches Lock Screen Explorer so that:

- Touch ID is offered only when `t1bridge-keybag.service` is active;
- a normal scan can still retry;
- three failures that each return in under one second pause Touch ID for the
  current lock session; and
- password authentication remains independent and available.

The installer supports the 1.5 and 1.7 plugin layouts, applies nothing while
the session is locked, and fails without changing the file if a future plugin
version no longer matches either reviewed patch. Enterprise applies the same
patch to Voyager's staged plugin copy so its periodic sync is idempotent.

The underlying failures are tracked upstream:

- [Omarchy #9905](https://github.com/omacom/omarchy/issues/9905) — unbounded
  250 ms fingerprint retries.
- [T1Bridge #14](https://github.com/standardagents/t1bridge/issues/14) —
  sustained keybag-relay failures. T1Bridge 0.1.6 added systemd restart
  backoff; 0.1.7 added safe diagnostics but does not repair the SEP failure.

This guard targets the installed Lock Screen Explorer clone. The stock
Omarchy lock service under `/usr/share/omarchy` is package-owned and is never
edited by Leapfrog.
