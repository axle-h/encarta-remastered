import { createContext, useContext } from "react";
import type { AssocItem } from "../api/types";

/** The current article's media sequence, so any media box opens the viewer on the whole list. */
export const SequenceContext = createContext<AssocItem[] | undefined>(undefined);

export function useArticleSequence(): AssocItem[] | undefined {
  return useContext(SequenceContext);
}
