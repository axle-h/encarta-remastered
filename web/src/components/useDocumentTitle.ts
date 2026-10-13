import { useEffect } from "react";

const BASE = "Encarta";

export function useDocumentTitle(title: string | undefined | null) {
  useEffect(() => {
    document.title = title ? `${title} - ${BASE}` : BASE;
  }, [title]);
}
