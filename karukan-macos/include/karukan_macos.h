#ifndef KARUKAN_MACOS_H
#define KARUKAN_MACOS_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque engine handle */
typedef struct KarukanMacEngine KarukanMacEngine;

/* Lifecycle */
KarukanMacEngine *karukan_macos_engine_new(void);
int karukan_macos_engine_init(KarukanMacEngine *engine);
void karukan_macos_engine_free(KarukanMacEngine *engine);

/* Input */
int karukan_macos_process_key(
    KarukanMacEngine *engine,
    uint16_t keycode,
    uint32_t unicode_char,
    int has_unicode,
    int shift,
    int ctrl,
    int opt,
    int cmd,
    int is_press
);
void karukan_macos_reset(KarukanMacEngine *engine);
int karukan_macos_commit(KarukanMacEngine *engine);
void karukan_macos_save_learning(KarukanMacEngine *engine);
void karukan_macos_set_surrounding_text(
    KarukanMacEngine *engine,
    const char *text,
    unsigned int cursor_pos
);

/* Preedit queries */
int karukan_macos_has_preedit(const KarukanMacEngine *engine);
const char *karukan_macos_get_preedit(const KarukanMacEngine *engine);
unsigned int karukan_macos_get_preedit_len(const KarukanMacEngine *engine);
unsigned int karukan_macos_get_preedit_caret(const KarukanMacEngine *engine);

/* Commit queries */
int karukan_macos_has_commit(const KarukanMacEngine *engine);
const char *karukan_macos_get_commit(const KarukanMacEngine *engine);
unsigned int karukan_macos_get_commit_len(const KarukanMacEngine *engine);

/* Candidate queries */
int karukan_macos_has_candidates(const KarukanMacEngine *engine);
int karukan_macos_should_hide_candidates(const KarukanMacEngine *engine);
unsigned int karukan_macos_get_candidate_count(const KarukanMacEngine *engine);
const char *karukan_macos_get_candidate(const KarukanMacEngine *engine, unsigned int index);
const char *karukan_macos_get_candidate_annotation(const KarukanMacEngine *engine, unsigned int index);
unsigned int karukan_macos_get_candidate_cursor(const KarukanMacEngine *engine);

/* Aux text queries */
int karukan_macos_has_aux(const KarukanMacEngine *engine);
const char *karukan_macos_get_aux(const KarukanMacEngine *engine);

/* State */
int karukan_macos_input_mode(const KarukanMacEngine *engine);

#ifdef __cplusplus
}
#endif

#endif /* KARUKAN_MACOS_H */
