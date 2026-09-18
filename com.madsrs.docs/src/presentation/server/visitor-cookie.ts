export const visitorCookieName = "mads_docs_visitor";

export function readCookie(request: Request, name: string): string | undefined {
  const prefix = `${name}=`;
  const pair = request.headers
    .get("cookie")
    ?.split(";")
    .map((value) => value.trim())
    .find((value) => value.startsWith(prefix));
  return pair ? decodeURIComponent(pair.slice(prefix.length)) : undefined;
}
