/** A single source page referenced in a wiki query answer. */
export interface QuerySource {
  slug: string;
  title: string;
  relevance_score: number;
  snippet: string;
}
