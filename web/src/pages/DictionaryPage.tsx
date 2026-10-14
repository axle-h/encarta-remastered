import { useEffect, useRef, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { ApiError, api } from "../api/client";
import type { DictEntry, DictionaryInfo, DictLookupEntry } from "../api/types";
import { useApi } from "../api/useApi";
import { SpeakerButton, pronunciationSrc } from "../article/Pronunciation";
import { Html } from "../components/Html";
import { Icon } from "../components/Icons";
import { useDocumentTitle } from "../components/useDocumentTitle";
import { expandGrammarCodes } from "./dictCodes";
import { WHEEL_BEFORE, wheelRows } from "./wordWheel";

type Tab = "dictionary" | "thesaurus" | "bilingual";

const TAB_DICT: Record<Exclude<Tab, "bilingual">, string> = { dictionary: "DICTA", thesaurus: "THESA" };
const TAB_LABEL: Record<Tab, string> = { dictionary: "Dictionary", thesaurus: "Thesaurus", bilingual: "Translations" };

/** `ENG_FRA` → `FRA_ENG`. */
export function reversePair(name: string): string {
  const m = /^([A-Z]+)_([A-Z]+)$/.exec(name);
  return m ? `${m[2]}_${m[1]}` : name;
}

function tabOf(dict: string, infos: DictionaryInfo[] | undefined): Tab {
  const k = infos?.find((d) => d.name === dict)?.kind;
  if (k === "thesaurus" || dict === "THESA") return "thesaurus";
  if (k === "bilingual" || /_/.test(dict)) return "bilingual";
  return "dictionary";
}

/**
 * Encarta Dictionaries: English dictionary, thesaurus and eight bilingual dictionaries, with a
 * word wheel on the left and the entry on the right (api.md "Dictionary").
 */
export function DictionaryPage({ tab: routeTab, kids = false }: { tab: Tab; kids?: boolean }) {
  const params = useParams();
  const [search, setSearch] = useSearchParams();
  const navigate = useNavigate();
  const infos = useApi((s) => api.dictionaries(s), []);
  const entryDict = params.dict;
  const entryId = params.entry ? Number(params.entry) : undefined;
  const tab: Tab = entryDict ? tabOf(entryDict, infos.data?.dictionaries) : routeTab;
  const bilinguals = (infos.data?.dictionaries ?? []).filter((d) => d.kind === "bilingual" && d.entry_count > 0);
  const pair = search.get("pair") ?? (tab === "bilingual" && entryDict ? entryDict : "ENG_FRA");
  const dict = tab === "bilingual" ? pair : TAB_DICT[tab];
  const q = search.get("q") ?? "";
  const [draft, setDraft] = useState(q);
  useEffect(() => {
    setDraft(q);
  }, [q]);
  useDocumentTitle(`${TAB_LABEL[tab]}${q ? `: ${q}` : ""}`);

  const pre = kids ? "/kids" : "";
  const tabPath = (t: Tab) => (kids ? `${pre}/dictionary` : t === "dictionary" ? "/dictionary" : t === "thesaurus" ? "/thesaurus" : "/translate");
  const lookupPath = (t: Tab, word: string, p?: string) => {
    const sp = new URLSearchParams();
    if (word) sp.set("q", word);
    if (t === "bilingual" && p) sp.set("pair", p);
    const s = sp.toString();
    return `${tabPath(t)}${s ? `?${s}` : ""}`;
  };

  // Word wheel: the headwords around the typed text (eight before it, then the following ones).
  const [wheelQ, setWheelQ] = useState(q);
  useEffect(() => {
    setWheelQ(q);
  }, [q]);
  useEffect(() => {
    const t = window.setTimeout(() => setWheelQ(draft.trim()), 150);
    return () => window.clearTimeout(t);
  }, [draft]);
  const wheel = useApi(wheelQ ? (s) => api.dictLookup({ q: wheelQ, dict, around: WHEEL_BEFORE, limit: 80 }, s) : null, [wheelQ, dict]);
  const rows = wheel.data ? wheelRows(wheel.data.before ?? [], wheel.data.entries) : [];
  const wheelRef = useRef<HTMLUListElement>(null);
  useEffect(() => {
    wheelRef.current?.scrollTo(0, 0);
  }, [wheel.data]);

  // The entry shown: an explicit one, else the first exact match of the query.
  const exact = useApi(!entryId && q ? (s) => api.dictLookup({ q, dict, limit: 20 }, s) : null, [q, dict, entryId]);
  const firstExact = exact.data?.entries.find((e) => e.headword.toLowerCase() === q.toLowerCase()) ?? exact.data?.entries[0];
  const shownDict = entryId ? entryDict! : firstExact?.dictionary;
  const shownId = entryId ?? firstExact?.ref_id;

  const openEntry = (e: DictLookupEntry) => {
    navigate(`${pre}/dictionary/${e.dictionary}/${e.ref_id}?q=${encodeURIComponent(e.headword)}`);
  };

  return (
    <div className={`dict${kids ? " kids" : ""}`}>
      <div className="dict-window">
        <div className="dict-head">
          <h1 className="dict-title">
            {tab === "bilingual" ? (
              <>
                <span className="dict-lang">{(bilinguals.find((b) => b.name === pair)?.title ?? "translation").toLowerCase()}:</span> translations
              </>
            ) : (
              <>
                <span className="dict-lang">english:</span> {tab}
              </>
            )}
          </h1>
          <form
            className="dict-search"
            role="search"
            onSubmit={(e) => {
              e.preventDefault();
              navigate(lookupPath(tab, draft.trim(), pair));
            }}
          >
            <span className="searchbox-field">
              <Icon name="search" size={16} className="searchbox-lens" />
              <input value={draft} onChange={(e) => setDraft(e.target.value)} aria-label="Look up a word" placeholder="Type a word" autoFocus spellCheck={false} />
            </span>
            <button type="submit" className="searchbox-go" aria-label="Look up">
              <Icon name="go" size={18} />
            </button>
          </form>
          {tab === "bilingual" && (
            <label className="dict-pair">
              Language
              <select value={pair} onChange={(e) => navigate(lookupPath("bilingual", q, e.target.value))}>
                {bilinguals.map((b) => (
                  <option key={b.name} value={b.name}>
                    {b.title}
                  </option>
                ))}
              </select>
            </label>
          )}
          {!kids && (
            <div className="dict-tabs" role="tablist">
              {(["dictionary", "thesaurus", "bilingual"] as Tab[]).map((t) => (
                <Link key={t} role="tab" aria-selected={t === tab} className={t === tab ? "current" : ""} to={lookupPath(t, q, pair)}>
                  {TAB_LABEL[t]}
                </Link>
              ))}
            </div>
          )}
        </div>
        <div className="dict-body">
          <ul className="dict-wheel" aria-label="Words" ref={wheelRef}>
            {rows.map((r, i) => (
              <li key={`${r.entry.dictionary}-${r.entry.ref_id}-${i}`} className={r.after ? undefined : "before"}>
                <button type="button" className={r.entry.ref_id === shownId ? "current" : ""} onClick={() => openEntry(r.entry)}>
                  {r.label}
                </button>
              </li>
            ))}
            {wheelQ && wheel.data && rows.length === 0 && <li className="enc-muted">No words near “{wheelQ}”.</li>}
          </ul>
          <div className="dict-entry">
            {shownDict && shownId ? (
              <EntryView dict={shownDict} id={shownId} fallbackWord={q} kids={kids} />
            ) : q && exact.data && exact.data.entries.length === 0 ? (
              <p className="enc-muted">“{q}” is not in this dictionary.</p>
            ) : !q ? (
              <ul className="dict-help">
                <li>Type a word and press Enter to look it up.</li>
                <li>Choose a word from the list to see its entry.</li>
                <li>Use the tabs to switch between the dictionary, thesaurus and translations.</li>
              </ul>
            ) : (
              <div className="enc-spinner" />
            )}
          </div>
        </div>
        <div className="dict-foot">
          {q && (
            <Link to={`${pre}/search?q=${encodeURIComponent(q)}`}>Look up in Encyclopedia</Link>
          )}
        </div>
      </div>
    </div>
  );
}

function EntryView({ dict, id, fallbackWord, kids }: { dict: string; id: number; fallbackWord: string; kids: boolean }) {
  const navigate = useNavigate();
  // `/dictionary/entry/{id}` names no dictionary (ref ids are unique across them); a link whose
  // guessed dictionary is wrong is retried by id alone.
  const res = useApi(
    async (s) => {
      if (dict === "entry") return api.dictEntryById(id, s);
      try {
        return await api.dictEntry(dict, id, s);
      } catch (e) {
        if (e instanceof ApiError && e.status === 404) return api.dictEntryById(id, s);
        throw e;
      }
    },
    [dict, id],
  );
  const box = useRef<HTMLDivElement>(null);
  const pre = kids ? "/kids" : "";

  // A cross-reference to an id that is not on the disc: look the word up instead.
  useEffect(() => {
    if (res.error instanceof ApiError && res.error.status === 404 && fallbackWord) {
      api.dictLookup({ q: fallbackWord, dict: dict === "entry" ? undefined : dict, limit: 5 }).then((r) => {
        const e = r.entries[0];
        if (e) navigate(`${pre}/dictionary/${e.dictionary}/${e.ref_id}?q=${encodeURIComponent(e.headword)}`, { replace: true });
      }, () => undefined);
    }
  }, [res.error, fallbackWord, dict, navigate, pre]);

  useEffect(() => {
    box.current?.scrollTo(0, 0);
  }, [id]);
  useEffect(() => {
    if (box.current) expandGrammarCodes(box.current);
  }, [res.data]);

  if (res.error) return <p className="enc-muted">This entry is not available.</p>;
  const e: DictEntry | undefined = res.data;
  if (!e) return <div className="enc-spinner" />;
  const src = pronunciationSrc(e);
  const onItemLink = (ref: number, _para: number | null, a: HTMLAnchorElement) => {
    // Links in dictionary HTML point at other entries: bilingual translations at the reverse pair.
    const target = e.kind === "bilingual" && a.classList.contains("enc-d-media") ? reversePair(e.dictionary) : e.dictionary;
    navigate(`${pre}/dictionary/${target}/${ref}?q=${encodeURIComponent(a.textContent ?? "")}`);
    return true;
  };
  return (
    <div className="dict-entry-inner" ref={box}>
      <h2 className="dict-hw">
        {e.headword}
        {src && <SpeakerButton src={src} label={`Pronounce ${e.headword}`} />}
      </h2>
      <Html className="dict-html" html={e.html} onItemLink={onItemLink} kids={kids} />
    </div>
  );
}
