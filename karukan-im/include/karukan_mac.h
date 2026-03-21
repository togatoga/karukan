/*
 * Karukan IME - macOS C FFI Header
 *
 * This header defines the C interface to the Karukan IME engine for macOS.
 * Use this to integrate Karukan with InputMethodKit (Swift/ObjC).
 *
 * Key differences from the Linux (fcitx5) header:
 * - process_key takes macOS key codes (Carbon kVK_*, NSEvent character/modifierFlags)
 * - Preedit caret is in characters (not bytes) for setMarkedText compatibility
 */

#ifndef KARUKAN_MAC_H
#define KARUKAN_MAC_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handle to a Karukan macOS engine instance */
typedef struct KarukanMacEngine KarukanMacEngine;

/* --- Lifecycle --- */

/*
 * Create a new Karukan macOS engine instance.
 * Returns a pointer to the engine, or NULL on failure.
 */
KarukanMacEngine* karukan_mac_engine_new(void);

/*
 * Initialize the kanji converter (loads the neural network model).
 * Returns 0 on success, -1 on failure.
 */
int karukan_mac_engine_init(KarukanMacEngine* engine);

/*
 * Destroy a Karukan macOS engine instance and free its resources.
 */
void karukan_mac_engine_free(KarukanMacEngine* engine);

/* --- Input --- */

/*
 * Process a macOS key event.
 *
 * Parameters:
 *   engine         - The engine instance
 *   keycode        - NSEvent.keyCode (Carbon virtual key code, e.g. kVK_ANSI_A = 0x00)
 *   character      - Unicode scalar from NSEvent.characters (0 if unavailable)
 *   modifier_flags - NSEvent.modifierFlags.rawValue
 *   is_key_down    - 1 for key-down, 0 for key-up
 *
 * Returns 1 if the key was consumed by the IME, 0 otherwise.
 */
int karukan_mac_process_key(
    KarukanMacEngine* engine,
    uint16_t keycode,
    uint32_t character,
    uint64_t modifier_flags,
    int is_key_down
);

/*
 * Reset the engine state, clearing any pending input.
 */
void karukan_mac_engine_reset(KarukanMacEngine* engine);

/*
 * Set the surrounding text context from the editor.
 *
 * Parameters:
 *   engine     - The engine instance
 *   text       - The surrounding text (null-terminated UTF-8)
 *   cursor_pos - Cursor position in characters (macOS convention)
 */
void karukan_mac_engine_set_surrounding_text(
    KarukanMacEngine* engine,
    const char* text,
    uint32_t cursor_pos
);

/* --- Preedit (composition) text --- */

int karukan_mac_engine_has_preedit(const KarukanMacEngine* engine);

const char* karukan_mac_engine_get_preedit(const KarukanMacEngine* engine);

/* Preedit text length in bytes */
uint32_t karukan_mac_engine_get_preedit_len(const KarukanMacEngine* engine);

/*
 * Get the preedit caret position in CHARACTERS (not bytes).
 * Use this value for NSRange in setMarkedText:selectedRange:replacementRange:.
 */
uint32_t karukan_mac_engine_get_preedit_caret(const KarukanMacEngine* engine);

/* --- Commit text --- */

int karukan_mac_engine_has_commit(const KarukanMacEngine* engine);

const char* karukan_mac_engine_get_commit(const KarukanMacEngine* engine);

uint32_t karukan_mac_engine_get_commit_len(const KarukanMacEngine* engine);

/* --- Candidates --- */

int karukan_mac_engine_has_candidates(const KarukanMacEngine* engine);

int karukan_mac_engine_should_hide_candidates(const KarukanMacEngine* engine);

uint32_t karukan_mac_engine_get_candidate_count(const KarukanMacEngine* engine);

const char* karukan_mac_engine_get_candidate(const KarukanMacEngine* engine, uint32_t index);

const char* karukan_mac_engine_get_candidate_annotation(const KarukanMacEngine* engine, uint32_t index);

uint32_t karukan_mac_engine_get_candidate_cursor(const KarukanMacEngine* engine);

/* --- Auxiliary text --- */

int karukan_mac_engine_has_aux(const KarukanMacEngine* engine);

const char* karukan_mac_engine_get_aux(const KarukanMacEngine* engine);

uint32_t karukan_mac_engine_get_aux_len(const KarukanMacEngine* engine);

/* --- Timing --- */

uint64_t karukan_mac_engine_get_last_conversion_ms(const KarukanMacEngine* engine);

uint64_t karukan_mac_engine_get_last_process_key_ms(const KarukanMacEngine* engine);

/* --- Learning cache --- */

void karukan_mac_engine_save_learning(KarukanMacEngine* engine);

/* --- State --- */

int karukan_mac_engine_is_empty(const KarukanMacEngine* engine);

int karukan_mac_engine_commit(KarukanMacEngine* engine);

#ifdef __cplusplus
}
#endif

#endif /* KARUKAN_MAC_H */
