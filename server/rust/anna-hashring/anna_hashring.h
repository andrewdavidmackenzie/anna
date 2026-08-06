#ifndef ANNA_HASHRING_H
#define ANNA_HASHRING_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Opaque handle to a consistent hash ring.
 */
typedef struct AnnaHashRing AnnaHashRing;

/**
 * Result of a server lookup: public IP, private IP, thread ID.
 */
typedef struct ServerInfo {
  char *public_ip;
  char *private_ip;
  uint32_t tid;
} ServerInfo;

/**
 * Create a new hash ring.
 *
 * `global`: if true, uses the global hasher (for cross-node key distribution).
 *           if false, uses the local hasher (for intra-node thread distribution).
 * `base_offset`: port base offset for address generation.
 */
struct AnnaHashRing *anna_hashring_new(bool global, uint32_t base_offset);

/**
 * Free a hash ring.
 */
void anna_hashring_free(struct AnnaHashRing *ring);

/**
 * Insert a server into the ring with `virtual_nodes` virtual entries.
 * Returns 0 on success, -1 if tid >= 50 (port group overflow).
 */
int32_t anna_hashring_insert(struct AnnaHashRing *ring,
                             const char *public_ip,
                             const char *private_ip,
                             uint32_t tid,
                             uint32_t virtual_nodes);

/**
 * Remove all entries for a server from the ring.
 */
void anna_hashring_remove(struct AnnaHashRing *ring,
                          const char *public_ip,
                          const char *private_ip,
                          uint32_t tid);

/**
 * Return the number of entries in the ring (including virtual nodes).
 */
uint32_t anna_hashring_size(const struct AnnaHashRing *ring);

/**
 * Find responsible servers for a key with `rep_count` replicas.
 * Returns the number of servers found. Results written to `out_servers`.
 * Caller must free strings via `anna_string_free`.
 */
uint32_t anna_responsible_servers(const struct AnnaHashRing *ring,
                                  const char *key,
                                  uint32_t rep_count,
                                  struct ServerInfo *out_servers,
                                  uint32_t max_results);

/**
 * Get all unique servers in the ring.
 * Returns count. Caller must free strings via `anna_string_free`.
 */
uint32_t anna_hashring_get_unique_servers(const struct AnnaHashRing *ring,
                                           struct ServerInfo *out_servers,
                                           uint32_t max_results);

/**
 * Find responsible local thread IDs for a key.
 * Returns 0 if the ring was created with `global = true` (wrong ring type).
 */
uint32_t anna_responsible_local(const struct AnnaHashRing *ring,
                                 const char *key,
                                 uint32_t rep_count,
                                 uint32_t *out_tids,
                                 uint32_t max_results);

/**
 * Free a string allocated by the library.
 */
void anna_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif /* ANNA_HASHRING_H */
