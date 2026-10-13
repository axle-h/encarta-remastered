/**
 * Grammar codes of the English dictionary (`PART-OF-SPEECH`, `SYNTAX-CODING`) and the words the
 * reader prints for them (as seen in the reference VM: "vi" → "intransitive verb").
 */
export const GRAMMAR_CODES: Record<string, string> = {
  n: "noun",
  npl: "plural noun",
  adj: "adjective",
  adv: "adverb",
  abbr: "abbreviation",
  v: "verb",
  vt: "transitive verb",
  vi: "intransitive verb",
  vti: "transitive and intransitive verb",
  vr: "reflexive verb",
  "aux v": "auxiliary verb",
  interj: "interjection",
  tdmk: "trademark",
  prep: "preposition",
  conj: "conjunction",
  pron: "pronoun",
  det: "determiner",
  contr: "contraction",
  "indef art": "indefinite article",
};

/** Replaces grammar codes under `root` with their full words. */
export function expandGrammarCodes(root: ParentNode): void {
  root.querySelectorAll<HTMLElement>(".enc-d-part-of-speech, .enc-d-syntax-coding").forEach((el) => {
    const code = (el.textContent ?? "").trim();
    const full = GRAMMAR_CODES[code];
    if (full) {
      el.title = code;
      el.textContent = full;
    }
  });
}
