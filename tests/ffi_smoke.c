/*
 * C ABI smoke test for libcurlrs.
 *
 * Exercises the FFI surface without making any network calls so this test is
 * reliable in CI: version/strerror strings, init/cleanup, options on an empty
 * handle, and the response getters returning zero/NULL when no response has
 * been performed.
 *
 * Build:
 *   cargo rustc --lib --release --crate-type staticlib
 *   cc tests/ffi_smoke.c -I include target/release/libcurlrs.a \
 *      -lpthread -ldl -lm -o ffi_smoke
 *   ./ffi_smoke
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "curlrs.h"

#define CHECK(cond)                                                          \
    do {                                                                     \
        if (!(cond)) {                                                       \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);  \
            return 1;                                                        \
        }                                                                    \
    } while (0)

int main(void) {
    /* version() / strerror() always return a non-NULL static string. */
    const char *ver = curlrs_version();
    CHECK(ver != NULL);
    CHECK(strncmp(ver, "curlrs/", 7) == 0);
    printf("version: %s\n", ver);

    for (int code = 0; code <= 7; code++) {
        const char *msg = curlrs_strerror(code);
        CHECK(msg != NULL);
        CHECK(strlen(msg) > 0);
    }
    /* Out-of-range code still returns a non-NULL string. */
    CHECK(curlrs_strerror(999) != NULL);

    /* NULL handle is rejected on every operation that takes one. */
    CHECK(curlrs_easy_reset(NULL) == CURLRSE_INVALID_HANDLE);
    CHECK(curlrs_easy_setopt_long(NULL, CURLRSOPT_TIMEOUT, 10) == CURLRSE_INVALID_HANDLE);
    CHECK(curlrs_easy_response_status(NULL) == 0);
    CHECK(curlrs_easy_response_header_count(NULL) == 0);
    CHECK(curlrs_easy_response_header(NULL, 0) == NULL);

    /* cleanup(NULL) is a no-op. */
    curlrs_easy_cleanup(NULL);

    CURLRS *h = curlrs_easy_init();
    CHECK(h != NULL);

    /* Options round-trip. */
    CHECK(curlrs_easy_setopt_str(h, CURLRSOPT_URL, "http://example.com")
          == CURLRSE_OK);
    CHECK(curlrs_easy_setopt_str(h, CURLRSOPT_CUSTOMREQUEST, "GET")
          == CURLRSE_OK);
    CHECK(curlrs_easy_setopt_str(h, CURLRSOPT_USERAGENT, "ffi-smoke/1.0")
          == CURLRSE_OK);
    CHECK(curlrs_easy_setopt_str(h, CURLRSOPT_HEADER, "X-Test: yes")
          == CURLRSE_OK);
    CHECK(curlrs_easy_setopt_long(h, CURLRSOPT_CONNECTTIMEOUT, 5) == CURLRSE_OK);
    CHECK(curlrs_easy_setopt_long(h, CURLRSOPT_TIMEOUT, 10) == CURLRSE_OK);

    /* Bad option codes are rejected without corrupting the handle. */
    CHECK(curlrs_easy_setopt_str(h, 999, "noop") == CURLRSE_UNKNOWN_OPTION);
    CHECK(curlrs_easy_setopt_long(h, 999, 0) == CURLRSE_UNKNOWN_OPTION);

    /* Bad value type for an option is also rejected. */
    CHECK(curlrs_easy_setopt_long(h, CURLRSOPT_URL, 0) == CURLRSE_INVALID_ARG);
    CHECK(curlrs_easy_setopt_str(h, CURLRSOPT_TIMEOUT, "10") == CURLRSE_INVALID_ARG);

    /* No perform yet -> no response data. */
    CHECK(curlrs_easy_response_status(h) == 0);
    CHECK(curlrs_easy_response_header_count(h) == 0);
    CHECK(curlrs_easy_response_header(h, 0) == NULL);

    const uint8_t *body = (const uint8_t *)0xdead;
    size_t len = 42;
    CHECK(curlrs_easy_response_body(h, &body, &len) == CURLRSE_OK);
    CHECK(body == NULL);
    CHECK(len == 0);

    /* response_body rejects NULL out-pointers. */
    CHECK(curlrs_easy_response_body(h, NULL, &len) == CURLRSE_INVALID_ARG);
    CHECK(curlrs_easy_response_body(h, &body, NULL) == CURLRSE_INVALID_ARG);

    /* reset() clears options and leaves the handle valid. */
    CHECK(curlrs_easy_reset(h) == CURLRSE_OK);
    CHECK(curlrs_easy_response_status(h) == 0);

    /* Unsupported scheme surfaces as CURLRSE_UNSUPPORTED on perform. */
    CHECK(curlrs_easy_setopt_str(h, CURLRSOPT_URL, "gopher://example.com/")
          == CURLRSE_OK);
    int rc = curlrs_easy_perform(h);
    /* gopher isn't supported by the easy API today (dispatch only knows
     * http/https). Either INVALID_ARG (URL parsing rejects in init) or
     * UNSUPPORTED is acceptable depending on dispatch order. */
    CHECK(rc == CURLRSE_UNSUPPORTED || rc == CURLRSE_INVALID_ARG);

    curlrs_easy_cleanup(h);
    printf("ffi_smoke: OK\n");
    return 0;
}
