# Prometheus exporter for FritzBox devices

**This is not an officially supported Google product.**

The AVM FritzBox exports metrics via UPNP. This exporter polls them whenever
Prometheus scrapes it. This can be used to monitor various metrics including
DSL link capacity, error counters as well as throughput and WiFi association
counts.

The exporter mostly autodiscovers a bunch of useful metrics.

## Contributions

Please see the [separate document](docs/contributing.md) for details.
