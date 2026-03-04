
import { Routes, Route, Link } from 'react-router-dom'
import DashboardGrid from './pages/DashboardGrid'
import ConnectionConfig from './pages/ConnectionConfig'
import ConnectionConfigNoSidebar from './pages/ConnectionConfigNoSidebar'
import SyncroDbPrd from './pages/SyncroDbPrd'
import SchemaDeepDive from './pages/SchemaDeepDive'
import SafeSyncModal from './pages/SafeSyncModal'
import MigrationScriptEditor from './pages/MigrationScriptEditor'

function App() {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100vh', width: '100vw' }}>
      <nav style={{ padding: '1rem', background: '#161B22', display: 'flex', gap: '1rem', borderBottom: '1px solid #30363D' }}>
        <Link to="/" style={{ color: '#0df2e3' }}>Dashboard</Link>
        <Link to="/config" style={{ color: '#0df2e3' }}>Config</Link>
        <Link to="/config-no-sidebar" style={{ color: '#0df2e3' }}>Config (No Sidebar)</Link>
        <Link to="/prd" style={{ color: '#0df2e3' }}>PRD</Link>
        <Link to="/schema" style={{ color: '#0df2e3' }}>Schema</Link>
        <Link to="/modal" style={{ color: '#0df2e3' }}>Modal</Link>
        <Link to="/editor" style={{ color: '#0df2e3' }}>Editor</Link>
      </nav>
      <div style={{ flex: 1, overflow: 'auto' }}>
        <Routes>
          <Route path="/" element={<DashboardGrid />} />
          <Route path="/config" element={<ConnectionConfig />} />
          <Route path="/config-no-sidebar" element={<ConnectionConfigNoSidebar />} />
          <Route path="/prd" element={<SyncroDbPrd />} />
          <Route path="/schema" element={<SchemaDeepDive />} />
          <Route path="/modal" element={<SafeSyncModal />} />
          <Route path="/editor" element={<MigrationScriptEditor />} />
        </Routes>
      </div>
    </div>
  )
}

export default App
