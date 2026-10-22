# Atlas 999 — Obsidian · 0.2.0

Compleet React + TypeScript + CSS-project met Vite. De goedgekeurde chatpagina is geïntegreerd in de bestaande Atlas 999-layout. Afbeeldingen, broncode en een gebouwde `dist/` zijn inbegrepen. Geen hostingdienst nodig om lokaal verder te ontwikkelen.

## Starten

Installeer Node.js 22.13 of hoger. Open deze map in CMD, PowerShell of een andere terminal:

```bash
npm install
npm run dev
```

Open het adres dat Vite toont. De pagina opent met het voorbeeldgesprek uit het chatontwerp. Klik links op **Een nieuw project** of **Nieuw gesprek** voor de bestaande model/startpagina.

```bash
npm run build
npm run preview
```

De eerste opdracht controleert TypeScript en maakt `dist/`. De tweede serveert die gebouwde pagina lokaal. Open `index.html` niet rechtstreeks via file://; gebruik de lokale server.

## Wat is inbegrepen

- Alle zes bovenste tabs: Chat, Data, Coding, Trading, Models, Settings.
- Het onderste submenu Dashboard / Chat.
- De bestaande startpagina en modelkeuze.
- Nieuwe chatpagina met rechts uitgelijnde gebruikersberichten, Atlas-logo, tijden, antwoordpaneel, callout, actieknoppen en bijlageknop.
- Responsieve telefoon- en tabletindeling; inklapbare zijbalk en horizontaal scrolbare bovennavigatie.
- Kopiëren naar het klembord (vereist toestemming/ondersteuning van de browser), lokale waardering van antwoorden, opnieuw genereren bij een echte runtime en een stopknop tijdens een verzoek.
- Tekst- en codebijlagen tot 2 MB; de gekozen tekst wordt aan het volgende bericht toegevoegd.
- Lokale gespreksopslag. Het meegeleverde voorbeeld wordt als voorbeeld behandeld en wordt nooit automatisch naar het model gestuurd.
- Losse adapter voor een OpenAI-compatibele lokale runtime. Java of een desktopbridge kan hier later op aansluiten.

Data, Coding, Trading en Dashboard zijn nog lege werkruimtes. Ze voeren geen fictieve taken uit. De voorbeeldstatus meldt eerlijk dat er geen model verbonden is.

## Bronstructuur

```text
src/
  main.tsx
  components/
    AtlasWorkspace.tsx    venster, navigatie, composer en modelinstellingen
    ChatThread.tsx        berichten, tijden, callout en antwoordacties
    AtlasMark.tsx         bewerkbaar SVG-logo
  lib/
    conversations.ts     gespreksgegevens en het expliciete voorbeeld
    runtime.ts           geïsoleerde model-API-adapter
public/
  assets/
    atlas.css            bestaande basislayout
    chat.css             nieuwe chatpagina en responsive regels
    obsidian-atlas.png    eerder decor zonder bediening
    chat-background.png  chatdecor zonder bediening
    design-specification.png       oorspronkelijke startpagina
    chat-design-specification.png  goedgekeurde chatmock-up
  favicon.svg
```

Alle berichten, knoppen, invoervelden en menu's zijn echte HTML-elementen. Het scherm is geen afbeelding met klikvlakken. De gedetailleerde illustratie is een achtergrondasset. Een CSS-masker bewaart waar mogelijk het vrije rechterdeel van het originele decor; de tekst en knoppen van de mock-up maken geen deel uit van de bediening.

## Een echt model aansluiten

Start je modelruntime apart en open Settings. Vul het API-adres, bijvoorbeeld `http://localhost:8080/v1`, en de echte model-ID in. De frontend controleert `GET /models`, en stuurt daarna gesprekken naar `POST /chat/completions`. Aanroepen bevatten alleen de ondersteunde velden role/content; tijden en opmaak blijven frontendgegevens.

Een GGUF-bestand kiezen leest alleen de bestandsnaam en uploadt het model niet. De browser start geen inference-engine. **Model laden** opent daarom de runtime-instellingen. De Java-backend/GGUF-loader is nog niet gebouwd.

De broncode ondersteunt nu volledige antwoorden zonder streaming; tijdens het verzoek kun je annuleren. Opnieuw genereren vervangt het gekozen antwoord en eventuele volgende berichten. Het voorbeeldgesprek blijft onveranderd; vanuit het voorbeeld begint een verzonden echt bericht een nieuw gesprek.

Voor een lokale HTTP-runtime kun je deze frontend het beste lokaal draaien. Een gehoste HTTPS-pagina kan lokale netwerktoestemming, CORS of andere browserbeperkingen tegenkomen. Sta alleen de concrete frontend-origin toe in de runtime; schakel beveiliging niet algemeen uit. Er is nog geen API-keybeheer toegevoegd.

## Visuele overeenkomst

De componenten zijn afgemeten aan de goedgekeurde afbeelding. Bij dezelfde compositiegrootte beginnen de drie berichtpanelen op y=174, y=302 en circa y=657. De positie van het antwoordpaneel is x=319, met circa 744 pixels breedte. De invoerbalk is circa 71 pixels hoog. Dit zijn meetcoördinaten, geen vaste schermresolutie: de CSS gebruikt relatieve maten en aparte regels voor kleine schermen.

Een volledige pixelidentiteit is niet gegarandeerd: de AI-afbeelding bevat geen oorspronkelijke font- of componentmetadata en browserletters worden afzonderlijk gerenderd. De voorbeeldstatus wijkt bewust af van de mock-up: er wordt geen verbonden model geclaimd zonder echte runtime.

## Gecontroleerd

TypeScript en productiebouw van dit zelfstandige project. Visuele controle op desktop, tablet en telefoon, inclusief vergelijking op het formaat van het ontwerp. Waarderingsknop, navigatie, voorbeeld/startpagina en de verbindingdialoog zijn in de browser gecontroleerd. De klembordactie meldt netjes als de browser het kopiëren blokkeert. Een echte lokale LLM-runtime was niet beschikbaar voor een volledige inference-test.
