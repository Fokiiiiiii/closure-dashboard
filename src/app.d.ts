declare global {
 namespace App {
  interface Locals { user: { id: string } | null; locale: import('#lib/messages.js').Locale }
  interface Error { message: string }
 }
}
export {};
