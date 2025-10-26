# FsPulse React + shadcn/ui Migration

**Created**: 2025-10-26
**Last Updated**: 2025-10-26 (Evening Session)
**Status**: In Progress - All Explore Tabs Implemented
**Branch**: feature/react-shadcn-migration

## Executive Summary

This document tracks the migration of FsPulse's web UI from a monolithic HTML/JavaScript dashboard to a modern React + TypeScript + shadcn/ui application. This migration aims to create a more maintainable, scalable, and developer-friendly codebase while preserving all existing functionality and improving the user experience.

### Key Technologies

- **React 19.1.1** - Modern React with latest features
- **TypeScript ~5.9.3** - Type safety and better developer experience
- **Vite 7.1.7** - Fast build tool and dev server
- **shadcn/ui** - High-quality, accessible React components built on Radix UI
- **Tailwind CSS 4.1.16** - Utility-first CSS framework
- **React Router 7.9.4** - Client-side routing
- **lucide-react** - Icon library

### Design Philosophy

1. **Component-Driven Architecture**: Build reusable, composable components
2. **Type Safety First**: Leverage TypeScript for better code quality
3. **Accessibility**: Use Radix UI primitives for ARIA-compliant components
4. **Performance**: Code-splitting, lazy loading, optimized bundles
5. **Developer Experience**: Fast HMR, ESLint, clear project structure
6. **API Compatibility**: Maintain 100% compatibility with existing backend APIs

---

## Project Structure

```
frontend/
├── src/
│   ├── components/          # Reusable React components
│   │   ├── ui/             # shadcn/ui components (generated)
│   │   │   ├── button.tsx
│   │   │   ├── card.tsx
│   │   │   ├── dialog.tsx
│   │   │   ├── input.tsx
│   │   │   ├── pagination.tsx
│   │   │   ├── table.tsx
│   │   │   └── tabs.tsx
│   │   ├── data-table/     # Data table specific components
│   │   │   └── FilterModal.tsx
│   │   ├── Header.tsx      # Global header with scan progress
│   │   └── Sidebar.tsx     # Collapsible navigation sidebar
│   ├── pages/              # Page-level components (routes)
│   │   ├── HomePage.tsx
│   │   ├── ExplorePage.tsx
│   │   └── explore/        # Explore sub-pages
│   │       └── RootsView.tsx
│   ├── lib/                # Utilities and shared code
│   │   ├── api.ts          # API client functions
│   │   ├── types.ts        # TypeScript type definitions
│   │   └── utils.ts        # Utility functions (cn, etc.)
│   ├── contexts/           # React contexts (future)
│   ├── hooks/              # Custom React hooks (future)
│   ├── App.tsx             # Root application component
│   ├── main.tsx            # Application entry point
│   └── index.css           # Global styles and Tailwind imports
├── public/                 # Static assets
├── index.html              # HTML template
├── package.json            # Dependencies and scripts
├── tsconfig.json           # TypeScript configuration
├── vite.config.ts          # Vite configuration
├── components.json         # shadcn/ui configuration
└── eslint.config.js        # ESLint configuration
```

---

## Current Implementation Status

### ✅ Completed Components

#### 1. **Application Shell**
- **File**: `src/App.tsx`
- **Features**:
  - React Router setup with BrowserRouter
  - Header + Sidebar + Main content layout
  - Route definitions for all main sections
  - Flexbox layout with proper overflow handling

#### 2. **Header Component**
- **File**: `src/components/Header.tsx`
- **Features**:
  - Sticky header with border bottom
  - FsPulse branding
  - **Light/Dark mode toggle** with Sun/Moon icons
  - Theme persistence via localStorage
  - System preference detection on first load
  - Placeholder for scan progress (future implementation)
  - Consistent height (h-16 / 64px)

#### 3. **Sidebar Component**
- **File**: `src/components/Sidebar.tsx`
- **Features**:
  - Collapsible navigation (64px collapsed, 200px expanded)
  - Hover to expand/collapse behavior
  - Active route highlighting with React Router's NavLink
  - Icons from lucide-react (Home, FolderSearch, Lightbulb, Database, Settings)
  - Smooth transitions for expand/collapse
  - Separated main nav items and settings
  - Proper visual feedback on hover

#### 4. **Home Page**
- **File**: `src/pages/HomePage.tsx`
- **Status**: Placeholder with theme test elements
- **Features**:
  - Basic layout structure
  - Color theme test elements
  - Ready for dashboard cards implementation

#### 5. **Explore Page**
- **File**: `src/pages/ExplorePage.tsx`
- **Features**:
  - Tabbed interface using shadcn/ui Tabs component
  - 6 tabs: Roots, Scans, Items, Changes, Alerts, Query
  - **All data tabs (Roots, Scans, Items, Changes, Alerts) fully functional**
  - Query tab remains placeholder (different implementation strategy)
  - All tabs use generic DataExplorerView component

#### 6. **Generic Data Explorer Component**
- **File**: `src/components/data-table/DataExplorerView.tsx`
- **Status**: **FULLY FUNCTIONAL** ✅
- **Purpose**: Reusable data table component for all domains
- **Features**:
  - **Two-panel layout**: Column selector (left) + Data table (right)
  - **Column Management**:
    - Drag-and-drop column reordering with HTML5 drag API
    - **Direction-aware drop indicator** (shows above/below based on drag direction)
    - Visual feedback during drag (opacity, highlight, blue line indicator)
    - Show/hide columns with checkboxes
    - Default columns loaded from backend metadata
    - Column position persistence in UI state
  - **Sorting**:
    - Three-state sort per column (none → asc → desc → none)
    - Sort indicators (↑ ↓ ⇅)
    - Single column sort (clicking new column clears previous)
    - Sort resets pagination to page 1
  - **Filtering**:
    - Filter modal with syntax hints from metadata
    - **Backend validation** before applying filters
    - Active filters display as inline badges with remove buttons
    - Filters apply to backend query
    - Filters reset pagination to page 1
  - **Data Display**:
    - Server-side pagination (25 items per page)
    - Column alignment from metadata (Left/Right/Center)
    - Responsive table with proper overflow
    - Hover states on table rows
    - Edge-to-edge card styling
    - ALL CAPS centered column headers
  - **Pagination**:
    - Simple Previous/Next buttons (shadcn components were incompatible with React Router)
    - Total count display ("Showing X to Y of Z")
    - Disabled state for prev/next at boundaries
  - **API Integration**:
    - Fetches metadata on mount (`/api/metadata/{domain}`)
    - Executes queries with column specs, filters, pagination (`/api/query/{domain}`)
    - Validates filters (`/api/validate-filter`)
    - Loading states
    - Error handling with user feedback

#### 7. **Domain-Specific Views**
- **Files**:
  - `src/pages/explore/RootsView.tsx`
  - `src/pages/explore/ScansView.tsx`
  - `src/pages/explore/ItemsView.tsx`
  - `src/pages/explore/ChangesView.tsx`
  - `src/pages/explore/AlertsView.tsx`
- **Status**: **ALL FUNCTIONAL** ✅
- **Implementation**: Thin wrappers around DataExplorerView
- **Pattern**: `<DataExplorerView domain="roots" />` (domain name is the only difference)

#### 8. **Filter Modal**
- **File**: `src/components/data-table/FilterModal.tsx`
- **Features**:
  - shadcn/ui Dialog component with enhanced visibility
  - **Backend filter validation** via `/api/validate-filter`
  - Filter syntax hints from column metadata
  - **Monospace error display** with preserved formatting (line breaks, indentation)
  - Async validation with loading state
  - Keyboard shortcuts (Enter to apply, Escape to cancel)
  - Clear error messaging with visual styling

#### 9. **Theme Hook**
- **File**: `src/hooks/useTheme.ts`
- **Features**:
  - Manages light/dark theme state
  - Checks localStorage for saved preference
  - Falls back to system preference (`prefers-color-scheme`)
  - Toggles `dark` class on document root
  - Persists theme choice to localStorage

#### 10. **API Client**
- **File**: `src/lib/api.ts`
- **Features**:
  - `fetchMetadata(domain)` - Get column metadata for any domain
  - `executeQuery(domain, request)` - Execute queries with filters/sorting/pagination
  - `validateFilter(request)` - **NEW**: Validate filter values before applying
  - Custom ApiError class with status codes
  - Centralized error handling
  - Type-safe responses

#### 11. **Type Definitions**
- **File**: `src/lib/types.ts`
- **Features**:
  - Complete TypeScript types matching backend API
  - ColumnMetadata, MetadataResponse
  - QueryRequest, QueryResponse
  - ValidateFilterRequest, ValidateFilterResponse - **NEW**
  - ColumnState (extends metadata with UI state)
  - ActiveFilter
  - Type aliases for SortDirection, Alignment, ColumnType

#### 12. **shadcn/ui Components** (Installed & Customized)
- **Files**: `src/components/ui/`
- **Components**:
  - Button - Multiple variants (default, outline, ghost, etc.)
  - Badge - Inline badges for active filters - **NEW**
  - Card - Card, CardHeader, CardTitle, CardContent
  - Dialog - **Enhanced**: Modal dialogs with backdrop blur and stronger borders
  - Input - Styled form inputs
  - Pagination - Pagination controls (not used - incompatible with React Router)
  - Table - Table primitives
  - Tabs - Tabbed interface with Radix UI

---

## Information Architecture

### Navigation Structure

```
┌─────────────────────────────────────────────────────┐
│  FsPulse                        [Scan Progress]     │ ← Header
├──────┬──────────────────────────────────────────────┤
│ [≡]  │                                              │
│ Home │                                              │
│ Scan │         Main Content Area                    │
│ Insig│         (Routes render here)                 │
│ Explo│                                              │
│ ─────│                                              │
│ Setti│                                              │
└──────┴──────────────────────────────────────────────┘
  ^                      ^
Sidebar               Content
(64px →               (flex-1,
 200px)               overflow)
```

### Routes

| Route       | Component      | Status      | Description                    |
|-------------|----------------|-------------|--------------------------------|
| `/`         | HomePage       | Placeholder | Dashboard with cards           |
| `/scans`    | ScansPage      | TODO        | Active scan + Root management  |
| `/insights` | InsightsPage   | TODO        | Alerts, Statistics, Changes    |
| `/explore`  | ExplorePage    | Partial     | Data explorer with tabs        |
| `/settings` | SettingsPage   | TODO        | Application settings           |

### Explore Tabs

| Tab      | Status      | Description                           |
|----------|-------------|---------------------------------------|
| Roots    | ✅ Complete | Root data table with full features    |
| Scans    | ✅ Complete | Scan history table with full features |
| Items    | ✅ Complete | File system items table               |
| Changes  | ✅ Complete | Change history table                  |
| Alerts   | ✅ Complete | Alerts data table with full features  |
| Query    | Placeholder | Text-based FsPulse query interface    |

---

## Data Table Pattern (Reusable Architecture)

**REFACTORED**: The data table implementation has been extracted into a **generic, reusable component** that eliminates code duplication.

### Generic Component Approach

All data domains (Roots, Scans, Items, Changes, Alerts) now use the same `DataExplorerView` component:

```typescript
// src/components/data-table/DataExplorerView.tsx
export function DataExplorerView({ domain }: { domain: string }) {
  // All state management, API calls, and rendering logic
  // Domain is passed as a prop - that's the only difference!
}

// Domain-specific views are now trivial wrappers:
// src/pages/explore/RootsView.tsx
export function RootsView() {
  return <DataExplorerView domain="roots" />
}

// src/pages/explore/ScansView.tsx
export function ScansView() {
  return <DataExplorerView domain="scans" />
}
// ... and so on for Items, Changes, Alerts
```

### Key Features

1. **Metadata-Driven**: Column definitions come from backend `/api/metadata/{domain}`
2. **Two-Panel Layout**: Column selector + Data table side-by-side
3. **Column Management**: Show/hide, drag-and-drop reordering with direction-aware indicators
4. **Sorting**: Single-column sort with three states (none → asc → desc)
5. **Filtering**: Modal-based filters with **backend validation** via `/api/validate-filter`
6. **Pagination**: Server-side pagination (25 items/page) with Previous/Next controls
7. **State Management**: React useState for all UI state
8. **API Integration**: Fetches metadata + data, validates filters, handles loading/error states

### Benefits of Generic Component

- ✅ **Zero code duplication** across domains
- ✅ **Single source of truth** for data table behavior
- ✅ **Consistent UX** across all domains
- ✅ **Easy to maintain** - fixes/enhancements apply everywhere
- ✅ **Type-safe** - TypeScript ensures correctness

---

## Design System

### Colors (Tailwind CSS Variables)

Using shadcn/ui's default theme with CSS variables:

```css
:root {
  --background: 0 0% 100%;           /* #ffffff */
  --foreground: 222.2 84% 4.9%;      /* Near black */
  --card: 0 0% 100%;                 /* White */
  --card-foreground: 222.2 84% 4.9%;
  --primary: 222.2 47.4% 11.2%;      /* Dark blue-grey */
  --primary-foreground: 210 40% 98%;
  --muted: 210 40% 96.1%;            /* Light grey */
  --muted-foreground: 215.4 16.3% 46.9%;
  --accent: 210 40% 96.1%;
  --accent-foreground: 222.2 47.4% 11.2%;
  --border: 214.3 31.8% 91.4%;       /* Subtle border */
  --input: 214.3 31.8% 91.4%;
  --ring: 222.2 84% 4.9%;
}
```

### Typography

- **Font Family**: System font stack (via Tailwind defaults)
- **Headings**:
  - H1: `text-2xl font-semibold` (24px, 600 weight)
  - H2: `text-xl font-semibold` (20px, 600 weight)
- **Body**: `text-base` (16px)
- **Small**: `text-sm` (14px)
- **Muted**: `text-muted-foreground` (grey text)

### Spacing

Using Tailwind's 4px-based spacing scale:
- **Tight**: `gap-1` (4px)
- **Default**: `gap-4` (16px)
- **Section**: `gap-6` (24px)
- **Page padding**: `p-6` (24px)

### Component Variants

**Button Variants** (from shadcn/ui):
- `default` - Primary solid button
- `outline` - Bordered button
- `ghost` - Transparent with hover
- `link` - Text-only link style

**Card** - Elevated container with border and padding

**Dialog** - Modal overlay with focus trap

---

## Backend API Integration

### API Endpoints Used

| Endpoint                | Method | Description                      |
|-------------------------|--------|----------------------------------|
| `/api/metadata/{domain}`| GET    | Column metadata for domain       |
| `/api/query/{domain}`   | POST   | Execute query with filters/sort  |

### Domain Types

- `roots` - Scan root directories
- `scans` - Scan history
- `items` - File system items
- `changes` - Change tracking data
- `alerts` - System alerts

### Request/Response Flow

1. **Mount**: Component fetches metadata
   ```typescript
   GET /api/metadata/roots
   → { domain: "roots", columns: [...] }
   ```

2. **User Interaction**: Filter, sort, paginate
   ```typescript
   POST /api/query/roots
   {
     columns: [{ name: "root_path", visible: true, sort_direction: "asc", position: 0 }],
     filters: [{ column: "root_path", value: ":(Cabinet)" }],
     limit: 25,
     offset: 0
   }
   → { columns: ["root_path", "root_id"], rows: [...], total: 42 }
   ```

3. **Display**: Render data in table with pagination controls

---

## Implementation Phases

### Phase 1: Foundation ✅ (COMPLETED)

**Goal**: Project setup, basic layout, and one working data table

**Tasks**:
1. ✅ Initialize Vite + React + TypeScript project
2. ✅ Install and configure shadcn/ui
3. ✅ Set up React Router
4. ✅ Create application shell (Header, Sidebar, main content)
5. ✅ Implement collapsible sidebar with navigation
6. ✅ Create Explore page with tabs
7. ✅ Fully implement Roots data table with all features
8. ✅ Create reusable API client
9. ✅ Define TypeScript types matching backend

**Deliverable**: ✅ Working app with functional Roots data table

---

### Phase 2: Explore Page Completion (NEXT)

**Goal**: Complete all Explore tabs using the data table pattern

**Priority Order**:
1. **Scans Tab** (scans domain)
   - Copy RootsView pattern
   - Implement ScansView component
   - Connect to `/api/metadata/scans` and `/api/query/scans`
   - Test sorting, filtering, pagination

2. **Items Tab** (items domain)
   - Implement ItemsView component
   - Handle large datasets (items can be millions of rows)
   - Optimize rendering performance

3. **Changes Tab** (changes domain)
   - Implement ChangesView component
   - Consider default filters (e.g., recent changes)

4. **Alerts Tab** (alerts domain)
   - Implement AlertsView component
   - Add alert status dropdown (Open/Flagged/Dismissed)
   - Consider quick action buttons (Dismiss, Flag)

5. **Query Tab** (text-based query interface)
   - Text area for FsPulse query language
   - Example queries dropdown
   - Execute button
   - Results table display
   - Query history (localStorage)

**Tasks**:
- [ ] Create ScansView component
- [ ] Create ItemsView component
- [ ] Create ChangesView component
- [ ] Create AlertsView component
- [ ] Create QueryView component with text editor
- [ ] Add example queries to Query tab
- [ ] Test all tabs with real data
- [ ] Implement localStorage persistence for query history

**Deliverable**: Explore page fully functional with all 6 tabs

---

### Phase 3: Scans Page

**Goal**: Active scan monitoring + Root management

**Layout**:
```
┌────────────────────────────────────────────────┐
│  Scans                                         │
├────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────┐ │
│  │  Active Scan Progress                    │ │
│  │  (or "Start New Scan" button when idle)  │ │
│  └──────────────────────────────────────────┘ │
│                                                │
│  ┌──────────────────────────────────────────┐ │
│  │  Roots                                   │ │
│  │  ┌────────────────────────────────────┐ │ │
│  │  │  Root Path  Last Scan   Items      │ │ │
│  │  ├────────────────────────────────────┤ │ │
│  │  │  /Cabinet   5 min ago   45,238     │ │ │
│  │  │  [Scan Now] [Edit] [Delete]        │ │ │
│  │  └────────────────────────────────────┘ │ │
│  │  [+ Add Root]                          │ │
│  └──────────────────────────────────────────┘ │
└────────────────────────────────────────────────┘
```

**Components to Build**:
1. **ScansPage** - Page wrapper
2. **ActiveScanCard** - Real-time scan progress
   - WebSocket connection for live updates
   - Expandable details (phases, threads, current file)
   - Cancel scan button
3. **RootsTable** - Root management table
   - List of configured roots
   - Actions per root (Scan Now, Edit, Delete)
   - Add root modal
4. **AddRootModal** - Dialog to add new root
   - Path picker
   - Scan options (hash, validate, etc.)

**Tasks**:
- [ ] Create ScansPage component
- [ ] Implement ActiveScanCard with WebSocket
- [ ] Create RootsTable component
- [ ] Create AddRootModal component
- [ ] Implement scan start/cancel actions
- [ ] Test real-time updates during scan

**Deliverable**: Scans page functional with root management and active scan monitoring

---

### Phase 4: Insights Page

**Goal**: Tabbed interface for Alerts, Statistics, and Changes analytics

**Layout**:
```
┌────────────────────────────────────────────────┐
│  Insights                                      │
├────────────────────────────────────────────────┤
│  [Alerts] [Statistics] [Changes]               │
│                                                │
│  Context: [All Data ▼] [Root: Cabinet ▼]      │
│                                                │
│  ┌──────────────────────────────────────────┐ │
│  │  Tab content (varies)                    │ │
│  └──────────────────────────────────────────┘ │
└────────────────────────────────────────────────┘
```

**Tabs**:
1. **Alerts Tab**:
   - Reuse AlertsView from Explore
   - Add context filtering
   - Add status dropdown per alert
   - Quick actions (Dismiss, Flag)

2. **Statistics Tab**:
   - Largest files (top 25 list)
   - File type distribution
   - Duplicate hash detection results
   - Items with validation issues

3. **Changes Tab**:
   - Recent changes list
   - Most frequently changed files
   - Recently deleted items
   - Items with hash changes

**Components to Build**:
1. **InsightsPage** - Page with tabs
2. **ContextFilter** - Dropdown to filter by All/Root/Scan
3. **StatisticsTab** - Statistics cards
4. **ChangesTab** - Changes analytics

**Tasks**:
- [ ] Create InsightsPage with tabs
- [ ] Implement context filtering
- [ ] Create Statistics tab with queries
- [ ] Create Changes tab with queries
- [ ] Add charts (optional - can use simple lists initially)

**Deliverable**: Insights page with 3 tabs and context filtering

---

### Phase 5: Home Page Dashboard

**Goal**: Functional dashboard with system overview and quick actions

**Layout**:
```
┌────────────────────────────────────────────────┐
│  Dashboard                                     │
├────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐           │
│  │ System Status│  │ Quick Scan   │           │
│  │ Total: 234K  │  │ □ Cabinet    │           │
│  │ Scans: 42    │  │ □ Documents  │           │
│  │ Roots: 3     │  │ [Scan]       │           │
│  └──────────────┘  └──────────────┘           │
│  ┌────────────────────────────────────────┐   │
│  │ Recent Activity                        │   │
│  │ • Scan completed: Cabinet (5 min ago)  │   │
│  │ • 3 new alerts (12 min ago)            │   │
│  └────────────────────────────────────────┘   │
│  ┌────────────────────────────────────────┐   │
│  │ Active Alerts (12)                     │   │
│  │ • 8 Hash changes                       │   │
│  │ • 4 Invalid items                      │   │
│  └────────────────────────────────────────┘   │
└────────────────────────────────────────────────┘
```

**Components to Build**:
1. **DashboardCard** - Reusable card component
2. **SystemStatusCard** - Metrics summary
3. **QuickScanCard** - Checkboxes + Scan button
4. **RecentActivityCard** - Timeline of events
5. **AlertsSummaryCard** - Alert counts

**Tasks**:
- [ ] Create dashboard cards
- [ ] Query backend for metrics
- [ ] Implement quick scan functionality
- [ ] Display recent activity timeline
- [ ] Link to Insights for alert details

**Deliverable**: Functional Home page dashboard

---

### Phase 6: Settings Page

**Goal**: Application configuration

**Sections**:
1. Server configuration (read-only display)
2. Scan defaults (threads, hash algorithm)
3. UI preferences (theme, default page)
4. Database management (backup, vacuum)

**Tasks**:
- [ ] Create SettingsPage component
- [ ] Implement settings API endpoints (if needed)
- [ ] Create settings forms
- [ ] Add validation
- [ ] Persist settings to backend

**Deliverable**: Functional Settings page

---

### Phase 7: Polish & Advanced Features

**Goal**: Production-ready UI with advanced features

**Tasks**:
1. **Performance Optimization**:
   - [ ] Implement code splitting (React.lazy)
   - [ ] Add loading skeletons
   - [ ] Optimize bundle size
   - [ ] Add query caching (React Query or similar)

2. **Accessibility**:
   - [ ] Keyboard navigation for all features
   - [ ] Screen reader labels
   - [ ] Focus management
   - [ ] ARIA attributes

3. **Error Handling**:
   - [ ] Error boundaries
   - [ ] Toast notifications
   - [ ] Retry mechanisms
   - [ ] Offline detection

4. **State Persistence**:
   - [ ] localStorage for column preferences
   - [ ] localStorage for filter presets
   - [ ] localStorage for query history
   - [ ] Session restoration

5. **Advanced Features**:
   - [ ] Export data (CSV, JSON)
   - [ ] Bulk operations (multi-select rows)
   - [ ] Saved query presets
   - [ ] Dark mode toggle
   - [ ] Customizable dashboard widgets

**Deliverable**: Production-ready React app

---

## Migration Strategy

### Development Workflow

1. **Parallel Development**: React app lives in `frontend/` directory, existing UI in `src/web/templates/`
2. **Shared Backend**: Both UIs use the same backend API
3. **Feature Parity**: Achieve 100% feature parity before switching
4. **Testing**: Thoroughly test React app with real data
5. **Gradual Rollout**: Deploy behind feature flag initially
6. **Deprecation**: Remove old UI once React app is stable

### Deployment Options

**Option A: Replace Existing UI**
- Build React app to static assets
- Serve from `/` (replace dashboard.html)
- Keep old UI at `/legacy` for fallback

**Option B: Dual Deployment**
- Serve React app from `/app` or `/ui`
- Keep old UI at `/` initially
- Switch default over time

**Option C: User Choice**
- Allow users to choose UI version in settings
- Set cookie/localStorage preference
- Both UIs available during transition

### Build Integration

**Vite Production Build**:
```bash
cd frontend
npm run build
# → Outputs to frontend/dist/
```

**Rust Integration**:
```rust
// In server.rs
// Serve React app static files
let frontend_dir = ServeDir::new("frontend/dist");
app = app.fallback_service(frontend_dir);
```

### API Compatibility

✅ **No backend changes required** - React app uses existing APIs:
- `/api/metadata/{domain}` - Already exists
- `/api/query/{domain}` - Already exists
- WebSocket endpoints - Already exist

New endpoints may be added for React-specific features but are not required for basic functionality.

---

## Testing Strategy

### Unit Tests (Future)

- Component testing with React Testing Library
- API client testing with mock fetch
- Utility function testing

### Integration Tests (Future)

- E2E testing with Playwright or Cypress
- Test full workflows (scan, filter, sort, etc.)
- Test API error scenarios

### Manual Testing Checklist

**Current Testing**:
- [x] Sidebar collapse/expand
- [x] Navigation between pages
- [x] Light/dark mode toggle
- [x] Theme persistence across sessions
- [x] All Explore tabs (Roots, Scans, Items, Changes, Alerts)
- [x] Column show/hide
- [x] Column drag-and-drop reordering with direction-aware indicators
- [x] Sorting (all three states)
- [x] Filtering with modal
- [x] Filter validation with backend
- [x] Monospace error display for validation errors
- [x] Active filter inline badges with removal
- [x] Pagination navigation
- [x] API error handling
- [x] Dialog modal visibility (backdrop blur)

**Future Testing**:
- [ ] Scans page with active scan
- [ ] Insights tabs with context filtering
- [ ] Home dashboard cards
- [ ] Settings page
- [ ] WebSocket real-time updates
- [ ] Offline behavior
- [ ] Mobile responsiveness (future)

---

## Backend API Additions

### New Endpoint: Filter Validation

**Endpoint**: `POST /api/validate-filter`

**Purpose**: Validate filter values before applying them to queries, using the same validation logic as the TUI.

**Implementation**:
- **File**: `src/web/handlers/query.rs:256-289`
- **Route**: Added to `src/web/server.rs:61`
- **Request**: `{ domain, column, value }`
- **Response**: `{ valid, error? }`
- **Validation**: Calls `QueryProcessor::validate_filter()` - same function used by TUI at `src/explore/filter_popup.rs:102`

**Benefits**:
- Prevents invalid queries from reaching the database
- Provides immediate, helpful error messages to users
- Reuses existing TUI validation logic (no duplication)
- Validates syntax, date formats, data types, etc.

**Example Error Format**:
```
 --> 1:1
  |
1 | test
  | ^---
  |
  = expected path_filter_EOI
```

---

## Key Implementation Decisions & Learnings

### 1. Generic Component Pattern

**Decision**: Extract data table logic into reusable `DataExplorerView` component.

**Rationale**:
- Eliminates code duplication across 5 domains
- Single source of truth for data table behavior
- Easier to maintain and enhance
- Consistent UX across all data views

**Implementation**: Domain passed as prop, all logic shared.

### 2. Direction-Aware Drag Indicators

**Problem**: Drag-and-drop indicator was confusing - showed top border when dragging down, bottom border when dragging up, but actual drop behavior was opposite.

**Solution**: Track drag direction by comparing dragged index vs target index:
- `draggedIndex < targetIndex` → dragging **down** → show indicator at **bottom**
- `draggedIndex > targetIndex` → dragging **up** → show indicator at **top**

**Code**: `src/components/data-table/DataExplorerView.tsx:104-109`

### 3. Filter Validation Architecture

**Decision**: Validate filters on backend before applying.

**Rationale**:
- Same validation logic as TUI (consistency)
- Prevents invalid queries from reaching database
- Better error messages than SQL errors
- Validates complex rules (dates, paths, etc.)

**Implementation**: Frontend calls `/api/validate-filter` before applying filter.

### 4. shadcn Pagination Incompatibility

**Problem**: shadcn Pagination components use `<a href="#">` which conflicts with React Router on page refresh, causing blank pages.

**Attempted Fix**: Modified pagination.tsx to support onClick handlers.

**Final Solution**: Use simple Tailwind-styled Previous/Next buttons instead of shadcn components.

**Lesson**: Some shadcn components are designed for traditional server-rendered apps with full page navigation, not SPAs with client-side routing.

### 5. Dialog Visibility in Dark Mode

**Problem**: Dialog modals were hard to see in dark mode - poor contrast with background.

**Solution**: Enhanced `src/components/ui/dialog.tsx`:
- **Backdrop blur**: `backdrop-blur-sm` on overlay (frosted glass effect)
- **Lighter overlay**: `bg-black/50` instead of `bg-black/80`
- **Thicker border**: `border-2` instead of `border`
- **Stronger shadow**: `shadow-xl` instead of `shadow-lg`

**Result**: Clear delineation between dialog and background in both light and dark modes.

### 6. Monospace Error Display

**Problem**: Backend validation errors contain formatted text with line breaks and indentation that were being collapsed.

**Solution**: Use `<pre>` tag with `whitespace-pre-wrap` and `font-mono` for error display.

**Result**: Parser errors display correctly with visual indicators and proper formatting.

### 7. Theme Implementation

**Decision**: Use simple localStorage + CSS class approach for theming.

**Implementation**:
- Custom `useTheme` hook manages state
- Adds/removes `dark` class on document root
- shadcn components use CSS variables that respond to `.dark` class
- Persists to localStorage, falls back to system preference

**Benefits**:
- Simple, no external dependencies
- Fast (no flashing on page load)
- Standard approach for shadcn/Tailwind

---

## Known Issues & Technical Debt

### Current Issues

None at this time - all known issues have been resolved.

### Technical Debt

1. **State Management**: Currently using useState. Consider migrating to:
   - React Query for server state
   - Zustand or Context for global UI state

2. ~~**Code Duplication**~~: **RESOLVED** ✅
   - Created generic `DataExplorerView` component
   - All domains now share single implementation

3. **Error Handling**: Basic error messages. Improve with:
   - Toast notifications (sonner or similar)
   - Error boundaries
   - Retry mechanisms

4. **Loading States**: Simple "Loading..." text. Improve with:
   - Skeleton loaders
   - Progressive loading
   - Optimistic updates

5. **Accessibility**: Good foundation with Radix UI. Audit with:
   - axe DevTools
   - Screen reader testing
   - Keyboard navigation testing

---

## Resolved Issues

### 1. shadcn Pagination Component + React Router Incompatibility (RESOLVED ✅)

**Problem**: The shadcn Pagination components (`PaginationPrevious`, `PaginationNext`, `PaginationLink`) are designed for `href`-based navigation and don't play well with React Router SPAs, causing blank pages on refresh.

**Attempted Solutions**:
1. ❌ Using onClick with href="#" - Still broke on refresh
2. ❌ Modifying pagination.tsx to support onClick - Complex TypeScript issues, still unreliable

**Final Solution**: Use custom Tailwind-styled pagination buttons instead of shadcn components
- Simple HTML `<button>` elements with Tailwind utility classes
- Clean, maintainable, and works perfectly with React Router
- Styled to match shadcn design system using CSS variables (`border-border`, `bg-accent`, `text-accent-foreground`, etc.)
- Proper disabled states and hover effects

**Result**: ✅ Pagination works perfectly, page refreshes work correctly, clean code, no complexity.

**Files Modified**:
- `src/pages/explore/RootsView.tsx` line 356-375 - Custom pagination buttons

**Decision**: Sometimes the simplest solution is best. The shadcn pagination components add unnecessary complexity for SPA use cases. Custom buttons give us full control and work reliably.

---

## Design Decisions

### 1. Why shadcn/ui instead of Material-UI or Ant Design?

**Decision**: Use shadcn/ui

**Rationale**:
- **Ownership**: Components are copied into your codebase, giving full control
- **Accessibility**: Built on Radix UI primitives (ARIA-compliant)
- **Styling**: Uses Tailwind CSS (consistent with our approach)
- **Bundle Size**: Only include components you use
- **Customization**: Easy to modify without fighting framework defaults

**Alternative Considered**: Material-UI, Ant Design (too opinionated, larger bundles)

### 2. TypeScript vs JavaScript

**Decision**: Use TypeScript

**Rationale**:
- **Type Safety**: Catch errors at compile time
- **Better DX**: Autocomplete, inline docs
- **API Compatibility**: Types match backend API exactly
- **Refactoring**: Safer refactors with type checking

### 3. State Management Approach

**Decision**: Start with React useState, migrate to React Query later

**Rationale**:
- **Simplicity**: useState is sufficient for current complexity
- **Server State**: React Query will handle API caching/sync later
- **Gradual Migration**: Easy to add React Query incrementally

**Future Migration Path**: useState → React Query (for API data) + Zustand (for UI state)

### 4. Reusable Data Table vs. Per-Domain Components

**Decision**: Per-domain components initially, refactor to generic later

**Rationale**:
- **Speed**: Copy-paste pattern gets all tabs working faster
- **Flexibility**: Each domain may have custom features
- **Refactor Later**: Extract common logic into generic component once patterns stabilize

**Future Refactor**: Create `<DataTableView domain="roots">` generic component

### 5. Sidebar: Fixed vs. Collapsible

**Decision**: Collapsible sidebar (64px → 200px on hover)

**Rationale**:
- **Space Efficiency**: More room for data tables
- **Consistency**: Matches existing UI behavior
- **Visual Clarity**: Icons + labels provide context

### 6. Client-Side Routing vs. Server-Side

**Decision**: Client-side routing with React Router

**Rationale**:
- **SPA Benefits**: Instant navigation, no full page reloads
- **State Preservation**: Maintain UI state during navigation
- **Better UX**: Smooth transitions, loading states

---

## Performance Considerations

### Bundle Size Targets

- Initial JS bundle: < 300 KB (gzipped)
- Total page weight: < 500 KB (initial load)
- Time to Interactive: < 2 seconds

### Optimization Techniques

1. **Code Splitting**: Lazy load routes with React.lazy
2. **Tree Shaking**: Vite automatically removes unused code
3. **Image Optimization**: Use WebP, lazy load images
4. **API Caching**: React Query will cache responses
5. **Virtual Scrolling**: For large tables (thousands of rows)

### Current Performance

- Vite HMR: < 100ms (instant updates)
- Production build: ~10 seconds
- Initial page load: < 1 second (local dev)

---

## Future Enhancements

### Short-term (Next 3 Months)

1. Complete all Explore tabs
2. Implement Scans page
3. Implement Insights page
4. Implement Home dashboard
5. Add Settings page
6. Achieve feature parity with old UI

### Medium-term (3-6 Months)

1. Dark mode support
2. Advanced filtering (AND/OR logic)
3. Saved filter presets
4. Export functionality (CSV, JSON)
5. Bulk operations (multi-select)
6. Charts and visualizations (Chart.js)

### Long-term (6+ Months)

1. Mobile-responsive design
2. Progressive Web App (PWA)
3. Offline support
4. Real-time collaboration (multiple users)
5. Advanced analytics
6. Customizable dashboards

---

## Resources & References

### Documentation

- [React Docs](https://react.dev)
- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/intro.html)
- [Vite Guide](https://vitejs.dev/guide/)
- [shadcn/ui Docs](https://ui.shadcn.com)
- [Tailwind CSS Docs](https://tailwindcss.com/docs)
- [Radix UI Primitives](https://www.radix-ui.com/primitives)

### Related FsPulse Documents

- `.claude/UI-DESIGN-PROPOSAL.md` - Original HTML/JS UI redesign
- `.claude/ARCHITECTURE.md` - Backend architecture
- `.claude/APPLE-DESIGN-SYSTEM.md` - Design principles

---

## Changelog

### 2025-10-26

**Phase 1 Complete** - Foundation implemented

**Added**:
- Project structure and build setup
- Header, Sidebar, application shell
- Explore page with tabs
- Fully functional Roots data table with:
  - Column management (show/hide, reorder)
  - Sorting (three-state per column)
  - Filtering (modal with syntax hints)
  - Pagination (server-side)
  - API integration (metadata + query)
- Filter modal component
- API client with error handling
- TypeScript types matching backend
- shadcn/ui components (Button, Card, Dialog, Input, Pagination, Table, Tabs)

**Next Steps**:
- Implement remaining Explore tabs (Scans, Items, Changes, Alerts, Query)
- Begin Phase 2: Explore Page Completion

---

**Document Version**: 1.0
**Last Updated**: 2025-10-26
**Author**: Claude (with human oversight)
**Status**: Foundation Complete, Phase 2 Starting
