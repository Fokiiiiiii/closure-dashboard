declare module 'node:crypto' {
 type Bytes = Uint8Array & { toString(encoding?: string): string };
 export function createHmac(algorithm: string, key: string): { update(data: string): { digest(): Bytes; digest(encoding: string): string } };
 export function randomBytes(size: number): Bytes;
 export function timingSafeEqual(a: Uint8Array, b: Uint8Array): boolean;
}
declare const Buffer: { from(value: string, encoding?: string): Uint8Array & { toString(encoding?: string): string } };
type Buffer = Uint8Array & { toString(encoding?: string): string };
