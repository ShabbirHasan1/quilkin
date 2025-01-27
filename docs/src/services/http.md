# HTTP

| Ports | Protocol            |
|-------|---------------------|
| 7600  | HTTP (IPv4 OR IPv6) |

Spawns a **HTTP** service, which exposes Quilkin's HTTP API for *game clients*,
for providing information that clients can use for features such as getting
a network coordinate system for measuring datacenter latency.

## Datacenter Latency

In addition to being able to ping Quilkin to get the latency between the client
and proxy. In order to allow clients to send information to services like a
matchmaker about which datacentre they are closest to, Quilkin also includes
the ability to get a proxy's latency to each of its connected datacentres.

> Note: This requires a multi-cluster relay setup, as when you set up proxies
  in the same cluster as gameservers, this measurement is redundant.

All that is required to set this up is to provide an ICAO code to the agent in
the gameserver cluster. (E.g. through the environment variable `ICAO_CODE`).
No further setup is required. **You can use duplicate ICAO codes**, Quilkin will
choose the best result amongst the duplicates to return. Quilkin assumes that
multiple of the same ICAO code refer to the same phyiscal datacentre, so latency
between them should negible.

> Why ICAO? ICAO is an international standard for airport codes, airport codes
  are an easy human readable code that makes it easy to use geo-visualisations
  in tools like Grafana, and easily allows grouping. IATA codes only cover
  major airports, ICAO codes cover practically every airport making them easy to
  more accurately represent the location of any datacentre.


### API And Schema

Currently the datacentre latency can be retrieved by sending a `GET /` HTTP
request to the QCMP port.

The returned data is a JSON object with each key being the ICAO code for the
datacentre, and the value being the latency in nanoseconds.

## Metrics

* `quilkin_phoenix_requests`

  The amount of phoenix (latency) requests

* `quilkin_phoenix_task_closed`

  Whether the phoenix latency measurement task has shutdown
  
* `quilkin_phoenix_server_errors`

  The amount of errors attempting to spawn the phoenix HTTP server

