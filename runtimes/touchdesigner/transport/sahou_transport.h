/* Sahou transport (zenoh) C ABI — implemented by libsahou_transport (cdylib).
 * The TD plugin bundles this dylib and calls these to actually put messages on the wire. */
#ifndef SAHOU_TRANSPORT_H
#define SAHOU_TRANSPORT_H

#ifdef __cplusplus
extern "C" {
#endif

/* Start the background zenoh peer session (idempotent). connect = optional explicit endpoint
 * like "tcp/127.0.0.1:7447" (also disables multicast); NULL = default peer (LAN multicast). */
void sahou_transport_start(const char* connect);

/* Queue one message to publish (non-blocking). No-op until started / on null args. */
void sahou_transport_publish(const char* key, const char* wire, const char* attachment);

/* Status JSON {"opened":bool,"sent":N,"error":"..."}; free with sahou_transport_free. */
char* sahou_transport_status(void);

/* Free a string returned by sahou_transport_status. NULL is a no-op. */
void sahou_transport_free(char* s);

#ifdef __cplusplus
}
#endif

#endif /* SAHOU_TRANSPORT_H */
