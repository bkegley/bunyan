import createClient from "openapi-fetch";
import type { paths } from "./schema";

const port = 3333;

export const api = createClient<paths>({
  baseUrl: `http://127.0.0.1:${port}`,
});
