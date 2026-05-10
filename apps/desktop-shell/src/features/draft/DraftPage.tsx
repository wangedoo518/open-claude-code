import { useLocation } from "react-router-dom";
import { DraftsPage } from "./DraftsPage";
import { DraftEditor } from "./DraftEditor";

/**
 * Slice E20.1 — sub-route splitter for `/drafts/*`.
 * `/drafts` (or `/drafts/`) → list, `/drafts/<slug>` → editor.
 *
 * Mirrors the KnowledgeHubPage pattern of pathname-parsing rather
 * than useParams — the route table mounts a single element per
 * `routePath` and we want to dispatch internally without touching
 * router config.
 */
export function DraftPage() {
  const slug = parseDraftSlug(useLocation().pathname);
  if (slug) return <DraftEditor slug={slug} />;
  return <DraftsPage />;
}

export function parseDraftSlug(pathname: string): string | null {
  // Match /drafts or /drafts/ → null; /drafts/<slug> → slug.
  const match = pathname.match(/^\/drafts\/([^/]+)\/?$/);
  if (!match) return null;
  const slug = decodeURIComponent(match[1]);
  return slug.length > 0 ? slug : null;
}
