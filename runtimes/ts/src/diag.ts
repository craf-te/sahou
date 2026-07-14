// Diagnostics and exceptions. A diagnostic is always {code, path, message} (produced by the core,
// byte-identical across all three languages).
export interface Diag {
  code: string;
  path: string;
  message: string;
}

export function fmtDiags(diags: Diag[]): string {
  return diags.map((d) => `[${d.code}] @${d.path}: ${d.message}`).join("; ");
}

export class SahouError extends Error {}

/** NO at a boundary (send boundary, or a fatal delivery failure). Structured diagnostics in `diags`. */
export class SahouRejected extends SahouError {
  readonly diags: Diag[];
  constructor(diags: Diag[]) {
    super(fmtDiags(diags));
    this.name = "SahouRejected";
    this.diags = diags;
  }
}

/** No response even after exhausting retries (possibly a transient failure; already retried). */
export class SahouUnreachable extends SahouError {
  constructor(
    readonly conn: string,
    readonly attempts: number,
  ) {
    super(`no response from connection '${conn}' (${attempts} attempts)`);
    this.name = "SahouUnreachable";
  }
}
