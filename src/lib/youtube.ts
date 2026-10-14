const ID = /(?:youtube\.com\/watch\?(?:[^#]*&)?v=|youtu\.be\/|youtube\.com\/shorts\/|youtube\.com\/embed\/|youtube\.com\/live\/)([A-Za-z0-9_-]{11})/;

export function extractYoutubeId(input: string): string | null {
  const text = input.trim();
  if (/^[A-Za-z0-9_-]{11}$/.test(text)) return text;
  return text.match(ID)?.[1] ?? null;
}

export function isYoutubeUrl(input: string): boolean {
  return extractYoutubeId(input) !== null;
}
