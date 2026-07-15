/* Sahou transport (zenoh) C ABI — implemented by libsahou_transport (cdylib).
 * The TD plugin bundles this dylib and calls these to actually put messages on the wire. */
#ifndef SAHOU_TRANSPORT_H
#define SAHOU_TRANSPORT_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Start the background zenoh peer session (idempotent). connect = optional explicit endpoint
 * like "tcp/127.0.0.1:7447" (also disables multicast); NULL = default peer (LAN multicast). */
void sahou_transport_start(const char* connect);

/* Queue one message to publish (non-blocking). No-op until started / on null args. */
void sahou_transport_publish(const char* key, const char* wire, const char* attachment);

/* Declare (ref-counted) a Zenoh subscriber for key, storing the latest sample. Start first.
 * Returns 1 when declared (or already present), 0 when the session is not open yet / declare failed
 * — the caller should retry on a later call (the background session opens asynchronously). Only a 1
 * counts as subscribed (a 0 does not bump the refcount). */
int sahou_transport_subscribe(const char* key);

/* Latest sample for key newer than since_generation, as JSON
 * {"generation":N,"wire":"...","attachment":"..."}, or "{}". Free with sahou_transport_free. */
char* sahou_transport_poll(const char* key, uint64_t since_generation);

/* Drop one subscription ref for key; undeclares the subscriber at zero. */
void sahou_transport_unsubscribe(const char* key);

/* Status JSON {"opened":bool,"sent":N,"error":"..."}; free with sahou_transport_free. */
char* sahou_transport_status(void);

/* Free a string returned by sahou_transport_status / sahou_transport_poll. NULL is a no-op. */
void sahou_transport_free(char* s);

#ifdef __cplusplus
}
#endif

#endif /* SAHOU_TRANSPORT_H */
