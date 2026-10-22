# Atlas 999 — filenames (bestand ↔ architectuur)

> Na **elke** werkbeurt bijwerken. Snel “waar hoort dit bestand?”.

## Root

| Bestand / map | Architectuur | Rol |
|---------------|--------------|-----|
| `Atlas_999_React.zip` | frontend (bronarchief) | Originele goedgekeurde UI-zip |
| `README.md` | meta | Korte projectintro |
| `docs/datanames.md` | docs | Architectuurkaart |
| `docs/filenames.md` | docs | Deze bestandsindex |
| `frontend/` | frontend | Actieve React UI |
| `backend/atlas-brain/` | atlas-brain | Rust Brain Core |

## Frontend

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `frontend/src/main.tsx` | frontend | React entry |
| `frontend/src/components/AtlasWorkspace.tsx` | frontend | Shell, navigatie, composer, settings, kennis-UI |
| `frontend/src/components/ChatThread.tsx` | frontend | Chatbubbels, acties, scroll |
| `frontend/src/components/AtlasMark.tsx` | frontend | SVG-merk |
| `frontend/src/lib/runtime.ts` | frontend ↔ atlas-brain | `/v1/models` + `/v1/chat/completions` adapter |
| `frontend/src/lib/brain.ts` | frontend ↔ config/memory/knowledge | Atlas control-plane client |
| `frontend/src/lib/conversations.ts` | frontend | Voorbeeldgesprek + typeguards |
| `frontend/public/assets/atlas.css` | frontend | Basislayout (pixel-spec) |
| `frontend/public/assets/chat.css` | frontend | Chat-geometry + settings/kennis add-ons |
| `frontend/public/assets/chat-background.png` | frontend | Chatdecor |
| `frontend/public/assets/chat-design-specification.png` | frontend | Goedgekeurde chat-mock referentie |
| `frontend/public/assets/obsidian-atlas.png` | frontend | Startpagina-decor |
| `frontend/public/assets/design-specification.png` | frontend | Startpagina-spec |
| `frontend/vite.config.ts` | frontend | Vite |
| `frontend/package.json` | frontend | Dependencies (React 19, Vite 8) |

## Backend — atlas-brain

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `backend/atlas-brain/Cargo.toml` | atlas-brain | Crate + deps |
| `backend/atlas-brain/src/main.rs` | atlas-brain | Boot, CORS, bind, graceful shutdown |
| `backend/atlas-brain/src/state.rs` | atlas-brain | Gedeelde AppState |
| `backend/atlas-brain/src/error.rs` | atlas-brain | Fouttype → HTTP JSON |
| `backend/atlas-brain/src/config.rs` | config-plane | brain-config load/save + PublicSettings |
| `backend/atlas-brain/src/api/mod.rs` | atlas-brain | Router (geen `/health`) |
| `backend/atlas-brain/src/api/models.rs` | local-lm-client | `GET /v1/models` |
| `backend/atlas-brain/src/api/chat.rs` | neural-pipeline + conversation-memory | `POST /v1/chat/completions` |
| `backend/atlas-brain/src/api/config_api.rs` | config-plane | `GET/PUT /v1/atlas/settings` |
| `backend/atlas-brain/src/api/conversations.rs` | conversation-memory | CRUD gesprekken |
| `backend/atlas-brain/src/api/knowledge.rs` | knowledge-store | learn / recall / list / delete |
| `backend/atlas-brain/src/lm/client.rs` | local-lm-client | HTTP naar lokale LM of stub |
| `backend/atlas-brain/src/brain/pipeline.rs` | neural-pipeline | SYSTEM → knowledge → turns → LM |
| `backend/atlas-brain/src/brain/recall.rs` | knowledge-store / neural | Lexical recall + knowledge block |
| `backend/atlas-brain/src/store/db.rs` | conversation-memory + knowledge-store | SQLite |
| `backend/atlas-brain/src/store/conversations.rs` | conversation-memory | Message/conversation types |
| `backend/atlas-brain/src/store/knowledge.rs` | knowledge-store | Knowledge types + request bodies |

## Runtime data (niet in git)

| Pad | Architectuur | Rol |
|-----|--------------|-----|
| `backend/atlas-brain/data/brain-config.json` | config-plane | Persistente settings incl. SYSTEM_PROMPT |
| `backend/atlas-brain/data/atlas.db` | memory + knowledge | SQLite database |
