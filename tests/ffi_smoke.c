/*
 * C ABI smoke test for librsurl.
 *
 * Exercises the FFI surface without making any network calls so this test is
 * reliable in CI: version/strerror strings, init/cleanup, options on an empty
 * handle, and the response getters returning zero/NULL when no response has
 * been performed.
 *
 * Build:
 *   cargo rustc --lib --release --crate-type staticlib
 *   cc tests/ffi_smoke.c -I include target/release/librsurl.a \
 *      -lpthread -ldl -lm -o ffi_smoke
 *   ./ffi_smoke
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "rsurl.h"

#define CHECK(cond)                                                          \
    do {                                                                     \
        if (!(cond)) {                                                       \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);  \
            return 1;                                                        \
        }                                                                    \
    } while (0)

int main(void) {
    /* version() / strerror() always return a non-NULL static string. */
    const char *ver = rsurl_version();
    CHECK(ver != NULL);
    CHECK(strncmp(ver, "rsurl/", 6) == 0);
    printf("version: %s\n", ver);

    for (int code = 0; code <= 7; code++) {
        const char *msg = rsurl_strerror(code);
        CHECK(msg != NULL);
        CHECK(strlen(msg) > 0);
    }
    /* Out-of-range code still returns a non-NULL string. */
    CHECK(rsurl_strerror(999) != NULL);

    /* NULL handle is rejected on every operation that takes one. */
    CHECK(rsurl_easy_reset(NULL) == RSURLE_INVALID_HANDLE);
    CHECK(rsurl_easy_setopt_long(NULL, RSURLOPT_TIMEOUT, 10) == RSURLE_INVALID_HANDLE);
    CHECK(rsurl_easy_response_status(NULL) == 0);
    CHECK(rsurl_easy_response_header_count(NULL) == 0);
    CHECK(rsurl_easy_response_header(NULL, 0) == NULL);

    /* cleanup(NULL) is a no-op. */
    rsurl_easy_cleanup(NULL);

    RSURL *h = rsurl_easy_init();
    CHECK(h != NULL);

    /* Options round-trip. */
    CHECK(rsurl_easy_setopt_str(h, RSURLOPT_URL, "http://example.com")
          == RSURLE_OK);
    CHECK(rsurl_easy_setopt_str(h, RSURLOPT_CUSTOMREQUEST, "GET")
          == RSURLE_OK);
    CHECK(rsurl_easy_setopt_str(h, RSURLOPT_USERAGENT, "ffi-smoke/1.0")
          == RSURLE_OK);
    CHECK(rsurl_easy_setopt_str(h, RSURLOPT_HEADER, "X-Test: yes")
          == RSURLE_OK);
    CHECK(rsurl_easy_setopt_long(h, RSURLOPT_CONNECTTIMEOUT, 5) == RSURLE_OK);
    CHECK(rsurl_easy_setopt_long(h, RSURLOPT_TIMEOUT, 10) == RSURLE_OK);

    /* Bad option codes are rejected without corrupting the handle. */
    CHECK(rsurl_easy_setopt_str(h, 999, "noop") == RSURLE_UNKNOWN_OPTION);
    CHECK(rsurl_easy_setopt_long(h, 999, 0) == RSURLE_UNKNOWN_OPTION);

    /* Bad value type for an option is also rejected. */
    CHECK(rsurl_easy_setopt_long(h, RSURLOPT_URL, 0) == RSURLE_INVALID_ARG);
    CHECK(rsurl_easy_setopt_str(h, RSURLOPT_TIMEOUT, "10") == RSURLE_INVALID_ARG);

    /* No perform yet -> no response data. */
    CHECK(rsurl_easy_response_status(h) == 0);
    CHECK(rsurl_easy_response_header_count(h) == 0);
    CHECK(rsurl_easy_response_header(h, 0) == NULL);

    const uint8_t *body = (const uint8_t *)0xdead;
    size_t len = 42;
    CHECK(rsurl_easy_response_body(h, &body, &len) == RSURLE_OK);
    CHECK(body == NULL);
    CHECK(len == 0);

    /* response_body rejects NULL out-pointers. */
    CHECK(rsurl_easy_response_body(h, NULL, &len) == RSURLE_INVALID_ARG);
    CHECK(rsurl_easy_response_body(h, &body, NULL) == RSURLE_INVALID_ARG);

    /* reset() clears options and leaves the handle valid. */
    CHECK(rsurl_easy_reset(h) == RSURLE_OK);
    CHECK(rsurl_easy_response_status(h) == 0);

    /* Unsupported scheme surfaces as RSURLE_UNSUPPORTED on perform. */
    CHECK(rsurl_easy_setopt_str(h, RSURLOPT_URL, "gopher://example.com/")
          == RSURLE_OK);
    int rc = rsurl_easy_perform(h);
    /* gopher isn't supported by the easy API today (dispatch only knows
     * http/https). Either INVALID_ARG (URL parsing rejects in init) or
     * UNSUPPORTED is acceptable depending on dispatch order. */
    CHECK(rc == RSURLE_UNSUPPORTED || rc == RSURLE_INVALID_ARG);

    rsurl_easy_cleanup(h);
    printf("ffi_smoke: OK\n");
    return 0;
}
