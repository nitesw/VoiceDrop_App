#ifndef VOICEDROP_CORE_H
#define VOICEDROP_CORE_H

/*
 * Hand-written to match core/src/lib.rs exactly. If cbindgen is introduced
 * later to generate this automatically, keep the function signatures below
 * as the source of truth for what Swift currently depends on.
 */

/* Returns an owned, NUL-terminated string. Caller must pass the result to
 * voicedrop_core_free_string exactly once. */
char *voicedrop_core_ping(void);

/* Frees a string previously returned by voicedrop_core_ping. Safe to call
 * with NULL. */
void voicedrop_core_free_string(char *s);

#endif /* VOICEDROP_CORE_H */
