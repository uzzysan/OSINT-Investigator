/**
 * Placeholder root page. Ticket 0001-04 proves the frontend delivery
 * mechanism end to end (Vite build embedded into the `vesica` binary and
 * served at `/`, with a dev-mode proxy to the running Rust server) — the
 * actual investigation UI (case list, graph canvas, timeline, ...) lands in
 * a later milestone. Deliberately minimal: the name, and nothing more.
 */
function App() {
  return (
    <main>
      <h1>Vesica</h1>
    </main>
  )
}

export default App
