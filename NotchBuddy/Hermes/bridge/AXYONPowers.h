// AXYON's superpowers inside the iPhone app: the computer's engine (windows/powers-ffi, a static library build-ios.sh
// builds into build/powers), answering Hermes' wealth_ and life_ tools (HermesArms.swift, HermesPowers).

#ifndef AXYONPowers_h
#define AXYONPowers_h

#include <stdbool.h>

/// One of Hermes' wealth_ or life_ tools: its `input` (a JSON object), the folder of the engine's files, the file of
/// the portfolio a paired computer gave (or ""), the iPhone's date (YYYY-MM-DD) and whether its evening has come (B3
/// publishes the day's quotes then). Blocks while the official sources are read (about a minute at most). Returns
/// `{"ok": bool, "text": string}`, to give back to `axyon_powers_free`; NULL only if it could not be written.
char *axyon_powers_call(const char *tool, const char *input, const char *folder, const char *copy, const char *today,
                        bool evening);

/// Gives back an answer of `axyon_powers_call`.
void axyon_powers_free(char *text);

#endif
