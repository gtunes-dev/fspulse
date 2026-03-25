#!/usr/bin/env python3
"""Generate fspulse test database (schema v29) with realistic integrity scenarios.

Creates a SQLite database matching the fspulse v29 schema (see src/db/schema/base.rs)
with synthetic data that exercises:
  - Folder hierarchy with correct item_id ordering (folders < contents)
  - File deletions and rehydrations
  - Correct hash mechanics (Baseline first, Suspect only after, no resets)
  - Deep suspect hash chains (21+ suspects on single item_versions)
  - Moderate suspect hash chains (3-10 suspects on ~8 items)
  - Cross-version suspect chains (suspects on multiple versions of same item)
  - High-churn items for version-browser pagination testing (45+ versions)
  - Realistic change rates (most files unchanged most scans)
  - Correct scan aggregate counts derived from actual data
  - Proper NULL val_state for unvalidated versions (not 0)
  - 30+ invalid validations across ~27 files
  - Tasks table with one task per scan

Output: tests/fixtures/fspulse.db
"""

import sqlite3, hashlib, os, random, json

random.seed(42)

ROOT_PATH = "Travel"
FILE, DIR = 0, 1
BASELINE, SUSPECT = 1, 2
VALID, INVALID = 1, 2

# ═══════════════════════════════════════════════════════════════
# Trip definitions → build ITEMS, CHILDREN, FOLDER_ORDER
# Each trip: (folder_name, subfolders_dict, files_list)
# files_list entries: (name, ext, has_validator, do_not_validate)
# ═══════════════════════════════════════════════════════════════

# Extension → (has_validator, default do_not_validate)
EXT_VALIDATORS = {
    'pdf': 1, 'jpg': 1, 'jpeg': 1, 'png': 1, 'tiff': 1,
    'docx': 0, 'xlsx': 0, 'flac': 0,
}

def file_entry(name, do_not_validate=0):
    """Build (name, ext, has_validator, do_not_validate) from filename."""
    ext = name.rsplit('.', 1)[1].lower() if '.' in name else None
    has_val = EXT_VALIDATORS.get(ext, 0) if ext else 0
    return (name, ext, has_val, do_not_validate)

# Trips in order.  Each is (folder_name, subfolders, main_files).
# subfolders: list of (subfolder_name, [file_entries])
TRIPS = [
    ("2022 - Patagonia", [], [
        file_entry("Glacier Photo.jpg"),
        file_entry("Hiking Trail Map.pdf"),
        file_entry("Hostel Booking.pdf"),
        file_entry("Patagonia Budget.xlsx"),       # no validator
        file_entry("Torres del Paine.png"),
        file_entry("Travel Insurance.pdf"),
        file_entry("Bus Tickets.pdf"),
        file_entry("Packing List.docx"),            # no validator
    ]),
    ("2023 - Greek Islands", [], [
        file_entry("Santorini Sunset.jpg"),
        file_entry("Ferry Schedule.pdf"),
        file_entry("Athens Hotel.pdf"),
        file_entry("Island Hopping Map.png"),
        file_entry("Restaurant Guide.docx"),        # no validator
        file_entry("Mykonos Beach.jpg", do_not_validate=1),
        file_entry("Greek Phrases.pdf"),
        file_entry("Aegean Cruise.pdf"),
    ]),
    ("2023 - Midnight Sun", [
        ("Flight Reservations", [
            file_entry("Chase Flight Reservation.pdf"),
            file_entry("Iceland Air.pdf"),
            file_entry("SAS Booking.pdf"),
        ]),
    ], [
        file_entry("Meeting Info.pdf"),
        file_entry("Midnight Sun Guide.pdf"),
        file_entry("NMS Travel Document 2023.pdf"),
        file_entry("Restaurant List.docx"),         # no validator
        file_entry("Northern Lights Photo.jpg"),
        file_entry("Tromsø Hotel.pdf"),
        file_entry("Arctic Gear List.pdf"),
    ]),
    ("2024 - Africa", [], [
        file_entry("Cape Town to Vic Falls.pdf"),
        file_entry("Sena 50S User Guide.pdf"),
        file_entry("Sunset Panorama.png"),
        file_entry("Vaccination Record.pdf"),
        file_entry("Safari Lodge Booking.pdf"),
        file_entry("Kruger Park Map.png"),
        file_entry("Table Mountain.jpg"),
        file_entry("Johannesburg Flight.pdf"),
        file_entry("Rand Exchange Rate.pdf"),
    ]),
    ("2024 - Japan", [], [
        file_entry("Tokyo Skyline.jpg"),
        file_entry("JR Pass Receipt.pdf"),
        file_entry("Kyoto Temple Guide.pdf"),
        file_entry("Sushi Restaurant List.docx"),   # no validator
        file_entry("Mt Fuji Photo.png"),
        file_entry("Shinkansen Schedule.pdf"),
        file_entry("Osaka Hotel.pdf"),
        file_entry("Japanese Phrases.pdf"),
        file_entry("Ryokan Booking.pdf"),
        file_entry("Nara Deer Park.jpg"),
    ]),
    ("2024 - New Zealand", [
        ("Susie", [
            file_entry("Passport Scan - John.jpg"),
            file_entry("Photo - John.jpeg"),
            file_entry("Visa - John.jpeg"),
        ]),
    ], [
        file_entry("Group Photo - Final.jpg"),
        file_entry("New Zealand Visa Receipt.jpg"),
        file_entry("Rental Agreement.pdf"),
        file_entry("Waiheke Ferry.pdf"),
        file_entry("Wire Transfer.pdf"),
        file_entry("Hobbiton Tickets.pdf"),
        file_entry("Milford Sound.jpg"),
    ]),
    ("2025 - Bohemian Rhapsody", [], [
        file_entry("Cathedral Photo.jpg", do_not_validate=1),
        file_entry("Currency Exchange.pdf"),
        file_entry("Emergency Contacts.pdf"),
        file_entry("Hotel Confirmations.pdf"),
        file_entry("Premera ID Cards.pdf"),
        file_entry("Route Map v3.png"),
        file_entry("Trip Itinerary Draft.pdf"),
        file_entry("Prague Castle.jpg"),
        file_entry("Charles Bridge.png"),
    ]),
    ("2025 - Mexico", [], [
        file_entry("Flight Confirmation AA.pdf"),
        file_entry("Market Scene.png"),
        file_entry("Oaxaca Itinerary.pdf"),
        file_entry("Packing Checklist.pdf"),
        file_entry("Souvenir Receipt.jpg", do_not_validate=1),
        file_entry("Travel Notes.docx"),            # no validator
        file_entry("Cenote Photo.jpg"),
        file_entry("Tulum Ruins.png"),
        file_entry("Playa del Carmen Hotel.pdf"),
        file_entry("Mayan Calendar.jpg"),
    ]),
    ("2025 - Scandinavia", [], [
        file_entry("Stockholm Skyline.jpg"),
        file_entry("Oslo Museum Pass.pdf"),
        file_entry("Copenhagen Hotel.pdf"),
        file_entry("Viking Ship Photo.png"),
        file_entry("Nordic Notes.docx"),            # no validator
        file_entry("Bergen Fjord.jpg"),
        file_entry("Helsinki Ferry.pdf"),
        file_entry("Lapland Tour.pdf"),
    ]),
    ("2026 - Heart of Africa", [], [
        file_entry("Insurance Documents.pdf"),
        file_entry("Visa Application.pdf"),
        file_entry("Safari Booking.pdf"),
        file_entry("Kilimanjaro Route.pdf"),
        file_entry("Zanzibar Beach.jpg"),
        file_entry("Serengeti Panorama.png"),
        file_entry("Maasai Village.jpg"),
    ]),
]

# Late arrivals: (filename, target_trip_folder, appear_scan, do_not_validate)
LATE_ARRIVALS = [
    ("Landscape.tiff",               "2024 - Africa",            5,  1),
    ("American Airlines Flights.pdf", "2025 - Mexico",            8,  0),
    ("ParkingPrint.pdf",              "2025 - Mexico",           15,  0),
    ("Audio Recording.flac",          "2025 - Mexico",           10,  0),
    ("Nairobi Guide.pdf",             "2026 - Heart of Africa",  18,  0),
    ("Bullet Train Selfie.jpg",       "2024 - Japan",            12,  0),
    ("Acropolis Ticket.pdf",          "2023 - Greek Islands",     6,  0),
    ("Aurora Borealis.jpg",           "2025 - Scandinavia",      20,  0),
    ("Penguin Colony.jpg",            "2022 - Patagonia",         3,  0),
    ("Queenstown Bungee.jpg",         "2024 - New Zealand",       7,  0),
    ("Train Schedule.pdf",            "2025 - Bohemian Rhapsody", 9,  0),
]


# ═══════════════════════════════════════════════════════════════
# Build ITEMS, CHILDREN, FOLDER_ORDER from TRIPS
# IDs assigned depth-first: folder, then subfolders (recursively), then main files
# ═══════════════════════════════════════════════════════════════
def build_items():
    """Return (ITEMS, CHILDREN, FOLDER_ORDER, name_to_id) lists."""
    items = []       # (id, rel_path, name, type, ext, has_validator, do_not_validate)
    children = {}    # folder_id -> [child_ids]
    name_to_id = {}  # filename -> item_id (for lookups)
    folder_name_to_id = {}  # folder_name -> folder_id
    next_id = [1]    # mutable counter

    def alloc():
        iid = next_id[0]
        next_id[0] += 1
        return iid

    for trip_name, subfolders, main_files in TRIPS:
        trip_id = alloc()
        items.append((trip_id, trip_name, trip_name, DIR, None, 0, 0))
        children[trip_id] = []
        folder_name_to_id[trip_name] = trip_id

        # Subfolders first (depth-first)
        for sf_name, sf_files in subfolders:
            sf_id = alloc()
            sf_path = f"{trip_name}/{sf_name}"
            items.append((sf_id, sf_path, sf_name, DIR, None, 0, 0))
            children[sf_id] = []
            children[trip_id].append(sf_id)
            folder_name_to_id[sf_path] = sf_id

            for fname, ext, has_val, dnv in sf_files:
                fid = alloc()
                fpath = f"{trip_name}/{sf_name}/{fname}"
                items.append((fid, fpath, fname, FILE, ext, has_val, dnv))
                children[sf_id].append(fid)
                name_to_id[fname] = fid

        # Main files
        for fname, ext, has_val, dnv in main_files:
            fid = alloc()
            fpath = f"{trip_name}/{fname}"
            items.append((fid, fpath, fname, FILE, ext, has_val, dnv))
            children[trip_id].append(fid)
            name_to_id[fname] = fid

    # Late arrivals (appended after all trips)
    for fname, target_folder, appear_scan, dnv in LATE_ARRIVALS:
        fid = alloc()
        ext = fname.rsplit('.', 1)[1].lower() if '.' in fname else None
        has_val = EXT_VALIDATORS.get(ext, 0) if ext else 0
        # Find the target folder
        target_id = folder_name_to_id[target_folder]
        fpath = f"{target_folder}/{fname}"
        items.append((fid, fpath, fname, FILE, ext, has_val, dnv))
        children[target_id].append(fid)
        name_to_id[fname] = fid

    # Compute FOLDER_ORDER: bottom-up by depth (deepest first)
    folder_ids = [i[0] for i in items if i[3] == DIR]
    # Depth = number of '/' in rel_path
    folder_depth = {}
    for i in items:
        if i[3] == DIR:
            folder_depth[i[0]] = i[1].count('/')
    folder_order = sorted(folder_ids, key=lambda fid: -folder_depth[fid])

    return items, children, folder_order, name_to_id, folder_name_to_id


ITEMS, CHILDREN, FOLDER_ORDER, NAME_TO_ID, FOLDER_NAME_TO_ID = build_items()

ITEMS_BY_ID = {i[0]: i for i in ITEMS}
FILE_IDS = [i[0] for i in ITEMS if i[3] == FILE]
FOLDER_IDS = [i[0] for i in ITEMS if i[3] == DIR]


# ═══════════════════════════════════════════════════════════════
# Scans: 50 scans, weekly spacing from Sep 19, 2024
# NO_HASH (is_hash=0): {7, 17, 27, 37, 47}
# hash_all=1: all EVEN-numbered hash scans
# is_val=1: every 3rd scan starting from 3
# ═══════════════════════════════════════════════════════════════
NO_HASH_SCANS = {7, 17, 27, 37, 47}
VAL_SCANS = {s for s in range(3, 51, 3)}  # {3,6,9,12,15,18,21,24,27,30,33,36,39,42,45,48}
HASH_ALL_SCANS = set()  # computed below

SCAN_DEFS = []
for s in range(1, 51):
    is_hash = 0 if s in NO_HASH_SCANS else 1
    is_val = 1 if s in VAL_SCANS else 0
    hash_all = 1 if (is_hash and s % 2 == 0) else 0
    SCAN_DEFS.append((s, is_hash, hash_all, is_val))
    if hash_all:
        HASH_ALL_SCANS.add(s)

# Verify: HASH_ALL_SCANS should be all even scans that are also hash scans
# Evens 2..50, minus those in NO_HASH = {2,4,6,8,10,12,14,16,18,20,22,24,26,28,30,32,34,36,38,40,42,44,46,48,50}
assert HASH_ALL_SCANS == {2,4,6,8,10,12,14,16,18,20,22,24,26,28,30,32,34,36,38,40,42,44,46,48,50}
assert len(HASH_ALL_SCANS) == 25

def scan_started_at(scan_id):
    return 1726704000 + (scan_id - 1) * 7 * 86400


# ═══════════════════════════════════════════════════════════════
# Resolve item IDs by name for explicit scenarios
# ═══════════════════════════════════════════════════════════════
def n(name):
    """Get item_id by filename."""
    return NAME_TO_ID[name]

# Key item IDs
ID_GLACIER_PHOTO      = n("Glacier Photo.jpg")
ID_HIKING_TRAIL_MAP   = n("Hiking Trail Map.pdf")
ID_CHASE_FLIGHT       = n("Chase Flight Reservation.pdf")
ID_CAPE_TOWN          = n("Cape Town to Vic Falls.pdf")
ID_MARKET_SCENE       = n("Market Scene.png")
ID_HOBBITON           = n("Hobbiton Tickets.pdf")
ID_OSAKA_HOTEL        = n("Osaka Hotel.pdf")
ID_SUNSET_PANORAMA    = n("Sunset Panorama.png")
ID_TOKYO_SKYLINE      = n("Tokyo Skyline.jpg")
ID_STOCKHOLM_SKYLINE  = n("Stockholm Skyline.jpg")
ID_SANTORINI_SUNSET   = n("Santorini Sunset.jpg")
ID_NORTHERN_LIGHTS    = n("Northern Lights Photo.jpg")
ID_MT_FUJI            = n("Mt Fuji Photo.png")
ID_ZANZIBAR_BEACH     = n("Zanzibar Beach.jpg")
ID_TABLE_MOUNTAIN     = n("Table Mountain.jpg")

# Late arrival name -> (item_id, appear_scan)
LATE_ARRIVAL_MAP = {}
for fname, target_folder, appear_scan, dnv in LATE_ARRIVALS:
    LATE_ARRIVAL_MAP[n(fname)] = appear_scan

# Set of all late arrival item IDs
LATE_ARRIVAL_IDS = set(LATE_ARRIVAL_MAP.keys())

# ═══════════════════════════════════════════════════════════════
# File lifecycle events: item_id -> [(scan_id, event)]
# ═══════════════════════════════════════════════════════════════

# Items with explicit lifecycle (not random)
EXPLICIT_IDS = set()

def build_file_events():
    events = {}

    # --- Deep suspect items: NEVER change ---
    events[ID_GLACIER_PHOTO] = [(1, 'appear')]
    events[ID_HIKING_TRAIL_MAP] = [(1, 'appear')]
    EXPLICIT_IDS.update([ID_GLACIER_PHOTO, ID_HIKING_TRAIL_MAP])

    # --- High-churn: Chase Flight Reservation changes every scan 2-45 ---
    events[ID_CHASE_FLIGHT] = [(1, 'appear')] + [(s, 'change') for s in range(2, 46)]
    EXPLICIT_IDS.add(ID_CHASE_FLIGHT)

    # --- Delete/rehydrate items ---
    # Cape Town to Vic Falls: changes 5,10, delete 12, rehydrate 25, changes 30,40
    events[ID_CAPE_TOWN] = [
        (1, 'appear'), (5, 'change'), (10, 'change'), (12, 'delete'),
        (25, 'appear'), (30, 'change'), (40, 'change'),
    ]
    EXPLICIT_IDS.add(ID_CAPE_TOWN)

    # Market Scene: change 5, delete 15, rehydrate 30, delete 40
    events[ID_MARKET_SCENE] = [
        (1, 'appear'), (5, 'change'), (15, 'delete'), (30, 'appear'), (40, 'delete'),
    ]
    EXPLICIT_IDS.add(ID_MARKET_SCENE)

    # Hobbiton Tickets: change 8, delete 20, rehydrate 35
    events[ID_HOBBITON] = [
        (1, 'appear'), (8, 'change'), (20, 'delete'), (35, 'appear'),
    ]
    EXPLICIT_IDS.add(ID_HOBBITON)

    # Osaka Hotel: change 5, delete 18, rehydrate 28, change 35
    events[ID_OSAKA_HOTEL] = [
        (1, 'appear'), (5, 'change'), (18, 'delete'), (28, 'appear'), (35, 'change'),
    ]
    EXPLICIT_IDS.add(ID_OSAKA_HOTEL)

    # --- Moderate suspect items: need specific change points ---
    # Sunset Panorama: change at 5 (before volatile period starts at 8)
    events[ID_SUNSET_PANORAMA] = [(1, 'appear'), (5, 'change')]
    EXPLICIT_IDS.add(ID_SUNSET_PANORAMA)

    # Tokyo Skyline: change at 3
    events[ID_TOKYO_SKYLINE] = [(1, 'appear'), (3, 'change')]
    EXPLICIT_IDS.add(ID_TOKYO_SKYLINE)

    # Stockholm Skyline: stable
    events[ID_STOCKHOLM_SKYLINE] = [(1, 'appear')]
    EXPLICIT_IDS.add(ID_STOCKHOLM_SKYLINE)

    # Santorini Sunset: change at 4
    events[ID_SANTORINI_SUNSET] = [(1, 'appear'), (4, 'change')]
    EXPLICIT_IDS.add(ID_SANTORINI_SUNSET)

    # Northern Lights Photo: stable
    events[ID_NORTHERN_LIGHTS] = [(1, 'appear')]
    EXPLICIT_IDS.add(ID_NORTHERN_LIGHTS)

    # Mt Fuji Photo: change at 5
    events[ID_MT_FUJI] = [(1, 'appear'), (5, 'change')]
    EXPLICIT_IDS.add(ID_MT_FUJI)

    # Zanzibar Beach: stable
    events[ID_ZANZIBAR_BEACH] = [(1, 'appear')]
    EXPLICIT_IDS.add(ID_ZANZIBAR_BEACH)

    # Table Mountain: change at 3
    events[ID_TABLE_MOUNTAIN] = [(1, 'appear'), (3, 'change')]
    EXPLICIT_IDS.add(ID_TABLE_MOUNTAIN)

    # --- Late arrivals: appear at their specified scan ---
    for fid, appear_scan in LATE_ARRIVAL_MAP.items():
        events[fid] = [(appear_scan, 'appear')]
        EXPLICIT_IDS.add(fid)

    # --- Random lifecycle for remaining files ---
    change_weights = [30, 25, 15, 10, 7, 5, 4, 3, 1]  # 0-8 changes
    for fid in FILE_IDS:
        if fid in EXPLICIT_IDS:
            continue
        # Regular file: appears at scan 1
        num_changes = random.choices(range(9), weights=change_weights, k=1)[0]
        change_scans = sorted(random.sample(range(2, 51), min(num_changes, 49)))
        events[fid] = [(1, 'appear')] + [(s, 'change') for s in change_scans]

    # --- Random changes for late arrivals (0-2 after appear) ---
    for fid, appear_scan in LATE_ARRIVAL_MAP.items():
        if fid in events and len(events[fid]) == 1:  # only appear event so far
            num_changes = random.choices(range(3), weights=[50, 35, 15], k=1)[0]
            if num_changes > 0 and appear_scan < 50:
                possible = list(range(appear_scan + 1, 51))
                change_scans = sorted(random.sample(possible, min(num_changes, len(possible))))
                events[fid] = events[fid] + [(s, 'change') for s in change_scans]

    return events


FILE_EVENTS = build_file_events()


# ═══════════════════════════════════════════════════════════════
# Hash volatility: item_id -> set of scan_ids where hash drifts
# (simulates bit rot on unchanged versions during hash_all scans)
# ═══════════════════════════════════════════════════════════════
VOLATILE = {
    # Deep suspects: volatile at ALL 25 hash_all scans → 25 suspects each on v1
    ID_GLACIER_PHOTO:    set(HASH_ALL_SCANS),
    ID_HIKING_TRAIL_MAP: set(HASH_ALL_SCANS),

    # Moderate suspects
    ID_SUNSET_PANORAMA:   {8, 14, 20, 26, 32, 38, 44, 50},    # 8 suspects on v2
    ID_TOKYO_SKYLINE:     {10, 18, 26, 34, 42, 50},            # 6 suspects on v2
    ID_STOCKHOLM_SKYLINE: {14, 22, 30, 38, 46},                # 5 suspects on v1
    ID_SANTORINI_SUNSET:  {8, 16, 28, 40},                     # 4 suspects on v2
    ID_NORTHERN_LIGHTS:   {6, 12, 20, 30, 40, 46, 50},         # 7 suspects on v1
    ID_MT_FUJI:           {10, 20, 30, 40, 50},                # 5 suspects on v2
    ID_ZANZIBAR_BEACH:    {16, 24, 32, 40, 48},                # 5 suspects on v1
    ID_TABLE_MOUNTAIN:    {8, 14, 20},                          # 3 suspects on v2

    # Cross-version suspects: volatile across multiple version lifetimes
    # Item 72 (Prague Castle.jpg): 9 versions — suspects on v2, v3, v5, v8
    72: {4, 6, 8,                                                  # v2 spans 2-8: 3 suspects
        10, 12, 14, 16,                                            # v3 spans 9-17: 4 suspects
        20, 22, 24, 26, 28, 30, 32, 34,                           # v5 spans 19-34: 8 suspects
        38, 40, 42, 44},                                           # v8 spans 37-44: 4 suspects
    # Item 26 (NMS Travel Document 2023.pdf): 8 versions — suspects on v2, v5
    26: {8, 10, 12, 14, 16, 18,                                   # v2 spans 7-19: 6 suspects
        28, 30, 32, 34, 36},                                       # v5 spans 26-36: 5 suspects
    # Item 93 (Lapland Tour.pdf): 8 versions — suspects on v1, v3, v6
    93: {4, 6, 8,                                                  # v1 spans 1-9: 3 suspects
        14, 16, 18, 20, 22, 24, 26, 28, 30,                       # v3 spans 12-31: 6 suspects (skip 12 for baseline)
        38, 40, 42},                                               # v6 spans 37-43: 3 suspects
}


# ═══════════════════════════════════════════════════════════════
# Invalid validations: (item_id, version) -> error string
# Versions must be alive during a val scan to get validated.
# Val scans: {3,6,9,12,15,18,21,24,27,30,33,36,39,42,45,48}
# ═══════════════════════════════════════════════════════════════
ERROR_STRINGS = [
    "Format error decoding Png: Invalid PNG signature.",
    "Format error decoding Png: Invalid color type 9.",
    "Format error decoding Png: Invalid bit depth 3.",
    "Format error decoding Png: IDAT or fdAT chunk is missing.",
    "Format error decoding Png: CRC error: expected 0x4353554d have 0x56112528 while decoding ChunkType { type: IHDR, critical: true, private: false, reserved: false, safecopy: false } chunk.",
    "failed parsing cross reference table: invalid start value",
    "IO error: failed to fill whole buffer",
    "couldn't parse input: invalid file header",
    "The encoder or decoder for Tiff does not support the color type `Unknown(0)`",
]

_err_idx = [0]
def next_error():
    e = ERROR_STRINGS[_err_idx[0] % len(ERROR_STRINGS)]
    _err_idx[0] += 1
    return e

# Build INVALID_AT
INVALID_AT = {}

# Deep suspect items (v1, validated at scan 3 since they appear at scan 1, no changes)
INVALID_AT[(ID_GLACIER_PHOTO, 1)] = next_error()
INVALID_AT[(ID_HIKING_TRAIL_MAP, 1)] = next_error()

# High-churn: Chase Flight changes every scan 2-45, so version N is created at scan N.
# Versions at val scans: version 3 (scan 3), version 6 (scan 6), etc.
# But the version number = scan_id for this item (appears scan 1, changes 2,3,...,45)
# So version at scan 3 = version 3, at scan 6 = version 6, etc.
# Pick 5 val scans for invalid: 6, 15, 24, 33, 42
for v in [6, 15, 24, 33, 42]:
    INVALID_AT[(ID_CHASE_FLIGHT, v)] = next_error()

# ~23 more items across different trips, mostly (item_id, 1) for stable items
# Pick files with has_validator=1 and do_not_validate=0
# We'll spread across trips
_additional_invalid_items = [
    n("Torres del Paine.png"),        # 2022-Patagonia (stable at v1 through scan 3)
    n("Travel Insurance.pdf"),        # 2022-Patagonia
    n("Bus Tickets.pdf"),             # 2022-Patagonia
    n("Ferry Schedule.pdf"),          # 2023-Greek Islands
    n("Athens Hotel.pdf"),            # 2023-Greek Islands
    n("Aegean Cruise.pdf"),           # 2023-Greek Islands
    n("Iceland Air.pdf"),             # 2023-Midnight Sun
    n("SAS Booking.pdf"),             # 2023-Midnight Sun
    n("Midnight Sun Guide.pdf"),      # 2023-Midnight Sun
    n("Sena 50S User Guide.pdf"),     # 2024-Africa
    n("Vaccination Record.pdf"),      # 2024-Africa
    n("Safari Lodge Booking.pdf"),    # 2024-Africa
    n("Kruger Park Map.png"),         # 2024-Africa
    n("JR Pass Receipt.pdf"),         # 2024-Japan
    n("Kyoto Temple Guide.pdf"),      # 2024-Japan
    n("Shinkansen Schedule.pdf"),     # 2024-Japan
    n("Rental Agreement.pdf"),        # 2024-NZ
    n("Wire Transfer.pdf"),           # 2024-NZ
    n("Waiheke Ferry.pdf"),           # 2024-NZ
    n("Currency Exchange.pdf"),       # 2025-Bohemian
    n("Hotel Confirmations.pdf"),     # 2025-Bohemian
    n("Premera ID Cards.pdf"),        # 2025-Bohemian
    n("Flight Confirmation AA.pdf"),  # 2025-Mexico
    n("Oaxaca Itinerary.pdf"),        # 2025-Mexico
    n("Oslo Museum Pass.pdf"),        # 2025-Scandinavia
]

for fid in _additional_invalid_items:
    # These are stable or have changes; version 1 is validated at first val scan (scan 3)
    INVALID_AT[(fid, 1)] = next_error()

# Verify we have 30+
assert len(INVALID_AT) >= 30, f"Only {len(INVALID_AT)} invalid entries, need 30+"


# ═══════════════════════════════════════════════════════════════
# Helpers
# ═══════════════════════════════════════════════════════════════
def make_hash(seed):
    return hashlib.sha256(seed.encode()).digest()

def gen_size(ext):
    ranges = {
        'pdf': (100_000, 5_000_000), 'jpg': (500_000, 10_000_000),
        'jpeg': (500_000, 8_000_000), 'png': (200_000, 8_000_000),
        'docx': (50_000, 2_000_000), 'xlsx': (50_000, 2_000_000),
        'tiff': (1_000_000, 20_000_000), 'flac': (5_000_000, 50_000_000),
    }
    lo, hi = ranges.get(ext, (10_000, 1_000_000))
    return random.randint(lo, hi)

def get_event(item_id, scan_id):
    for s, e in FILE_EVENTS.get(item_id, []):
        if s == scan_id:
            return e
    return None


# ═══════════════════════════════════════════════════════════════
# Simulation
# ═══════════════════════════════════════════════════════════════
def simulate():
    # State
    current_ver = {}   # item_id -> version number
    alive = {}         # item_id -> bool
    iv_rows = []       # mutable version dicts
    iv_idx = {}        # (item_id, ver) -> index
    hv_rows = []       # hash version dicts
    latest_hv = {}     # (item_id, ver) -> index in hv_rows
    scan_rows = []

    for scan_def in SCAN_DEFS:
        scan_id, is_hash, hash_all, is_val = scan_def
        started = scan_started_at(scan_id)
        changes = {}  # item_id -> 'added'|'modified'|'deleted'

        # --- File events ---
        for fid in FILE_IDS:
            item = ITEMS_BY_ID[fid]
            ext = item[4]
            event = get_event(fid, scan_id)

            if event == 'appear':
                ver = current_ver.get(fid, 0) + 1
                current_ver[fid] = ver
                alive[fid] = True
                iv = {
                    'item_id': fid, 'item_version': ver, 'root_id': 1,
                    'first_scan_id': scan_id, 'last_scan_id': scan_id,
                    'is_added': 1, 'is_deleted': 0, 'access': 0,
                    'mod_date': started - random.randint(60, 7 * 86400),
                    'size': gen_size(ext),
                    'add_count': None, 'modify_count': None,
                    'delete_count': None, 'unchanged_count': None,
                    'val_scan_id': None, 'val_state': None, 'val_error': None,
                    'val_reviewed_at': None, 'hash_reviewed_at': None,
                }
                iv_idx[(fid, ver)] = len(iv_rows)
                iv_rows.append(iv)
                changes[fid] = 'added'

            elif event == 'change':
                ver = current_ver[fid] + 1
                current_ver[fid] = ver
                alive[fid] = True
                prev = iv_rows[iv_idx[(fid, ver - 1)]]
                iv = {
                    'item_id': fid, 'item_version': ver, 'root_id': 1,
                    'first_scan_id': scan_id, 'last_scan_id': scan_id,
                    'is_added': 0, 'is_deleted': 0, 'access': 0,
                    'mod_date': started - random.randint(60, 7 * 86400),
                    'size': max(1000, prev['size'] + random.randint(-500000, 500000)),
                    'add_count': None, 'modify_count': None,
                    'delete_count': None, 'unchanged_count': None,
                    'val_scan_id': None, 'val_state': None, 'val_error': None,
                    'val_reviewed_at': None, 'hash_reviewed_at': None,
                }
                iv_idx[(fid, ver)] = len(iv_rows)
                iv_rows.append(iv)
                changes[fid] = 'modified'

            elif event == 'delete':
                ver = current_ver[fid] + 1
                current_ver[fid] = ver
                alive[fid] = False
                prev = iv_rows[iv_idx[(fid, ver - 1)]]
                iv = {
                    'item_id': fid, 'item_version': ver, 'root_id': 1,
                    'first_scan_id': scan_id, 'last_scan_id': scan_id,
                    'is_added': 0, 'is_deleted': 1, 'access': 0,
                    'mod_date': prev['mod_date'], 'size': prev['size'],
                    'add_count': None, 'modify_count': None,
                    'delete_count': None, 'unchanged_count': None,
                    'val_scan_id': None, 'val_state': None, 'val_error': None,
                    'val_reviewed_at': None, 'hash_reviewed_at': None,
                }
                iv_idx[(fid, ver)] = len(iv_rows)
                iv_rows.append(iv)
                changes[fid] = 'deleted'

            else:
                # No event — extend last_scan_id
                if fid in current_ver:
                    ver = current_ver[fid]
                    iv_rows[iv_idx[(fid, ver)]]['last_scan_id'] = scan_id

        # --- Folder versions (bottom-up) ---
        for folder_id in FOLDER_ORDER:
            child_ids = CHILDREN[folder_id]
            add_c = mod_c = del_c = unch_c = 0

            for cid in child_ids:
                ch = changes.get(cid)
                if ch == 'added':
                    add_c += 1
                elif ch == 'modified':
                    mod_c += 1
                elif ch == 'deleted':
                    del_c += 1
                elif alive.get(cid, False):
                    unch_c += 1

            has_change = (add_c + mod_c + del_c) > 0

            if folder_id not in current_ver:
                # First appearance (scan 1 for all folders)
                ver = 1
                current_ver[folder_id] = ver
                alive[folder_id] = True
                iv = {
                    'item_id': folder_id, 'item_version': ver, 'root_id': 1,
                    'first_scan_id': scan_id, 'last_scan_id': scan_id,
                    'is_added': 1, 'is_deleted': 0, 'access': 0,
                    'mod_date': started, 'size': None,
                    'add_count': add_c, 'modify_count': mod_c,
                    'delete_count': del_c, 'unchanged_count': unch_c,
                    'val_scan_id': None, 'val_state': None, 'val_error': None,
                    'val_reviewed_at': None, 'hash_reviewed_at': None,
                }
                iv_idx[(folder_id, ver)] = len(iv_rows)
                iv_rows.append(iv)
                changes[folder_id] = 'added'

            elif has_change:
                ver = current_ver[folder_id] + 1
                current_ver[folder_id] = ver
                iv = {
                    'item_id': folder_id, 'item_version': ver, 'root_id': 1,
                    'first_scan_id': scan_id, 'last_scan_id': scan_id,
                    'is_added': 0, 'is_deleted': 0, 'access': 0,
                    'mod_date': started, 'size': None,
                    'add_count': add_c, 'modify_count': mod_c,
                    'delete_count': del_c, 'unchanged_count': unch_c,
                    'val_scan_id': None, 'val_state': None, 'val_error': None,
                    'val_reviewed_at': None, 'hash_reviewed_at': None,
                }
                iv_idx[(folder_id, ver)] = len(iv_rows)
                iv_rows.append(iv)
                changes[folder_id] = 'modified'

            else:
                ver = current_ver[folder_id]
                iv_rows[iv_idx[(folder_id, ver)]]['last_scan_id'] = scan_id

        # --- Hashing ---
        if is_hash:
            for fid in FILE_IDS:
                if not alive.get(fid, False):
                    continue
                ver = current_ver[fid]
                hv_key = (fid, ver)

                should_hash = False
                if hash_all:
                    should_hash = True
                elif hv_key not in latest_hv:
                    should_hash = True

                if not should_hash:
                    continue

                prev_hash = hv_rows[latest_hv[hv_key]]['hash'] if hv_key in latest_hv else None

                if fid in VOLATILE and scan_id in VOLATILE[fid]:
                    new_hash = make_hash(f"rot:{fid}:{ver}:{scan_id}")
                elif prev_hash is not None:
                    new_hash = prev_hash
                else:
                    new_hash = make_hash(f"base:{fid}:{ver}")

                if hv_key not in latest_hv:
                    idx = len(hv_rows)
                    hv_rows.append({
                        'item_id': fid, 'item_version': ver,
                        'first_scan_id': scan_id, 'last_scan_id': scan_id,
                        'hash': new_hash, 'hash_state': BASELINE,
                    })
                    latest_hv[hv_key] = idx
                elif new_hash == prev_hash:
                    hv_rows[latest_hv[hv_key]]['last_scan_id'] = scan_id
                else:
                    idx = len(hv_rows)
                    hv_rows.append({
                        'item_id': fid, 'item_version': ver,
                        'first_scan_id': scan_id, 'last_scan_id': scan_id,
                        'hash': new_hash, 'hash_state': SUSPECT,
                    })
                    latest_hv[hv_key] = idx

        # --- Validation ---
        if is_val:
            for fid in FILE_IDS:
                if not alive.get(fid, False):
                    continue
                item = ITEMS_BY_ID[fid]
                has_val, dnv = item[5], item[6]
                if not has_val or dnv:
                    continue
                ver = current_ver[fid]
                iv = iv_rows[iv_idx[(fid, ver)]]
                if iv['val_state'] is not None:
                    continue
                key = (fid, ver)
                if key in INVALID_AT:
                    iv['val_state'] = INVALID
                    iv['val_error'] = INVALID_AT[key]
                else:
                    iv['val_state'] = VALID
                iv['val_scan_id'] = scan_id

        # --- Compute scan aggregates ---
        file_count = folder_count = total_size = 0
        val_unk = val_valid = val_invalid = val_noval = 0
        hash_unk = hash_base = hash_susp = 0

        for fid in FILE_IDS:
            if not alive.get(fid, False):
                continue
            file_count += 1
            ver = current_ver[fid]
            iv = iv_rows[iv_idx[(fid, ver)]]
            total_size += iv['size'] or 0

            item = ITEMS_BY_ID[fid]
            has_val = item[5]
            if not has_val:
                val_noval += 1
            elif iv['val_state'] is None:
                val_unk += 1
            elif iv['val_state'] == VALID:
                val_valid += 1
            elif iv['val_state'] == INVALID:
                val_invalid += 1

            hv_key = (fid, ver)
            if hv_key in latest_hv:
                hs = hv_rows[latest_hv[hv_key]]['hash_state']
                if hs == BASELINE:
                    hash_base += 1
                else:
                    hash_susp += 1
            else:
                hash_unk += 1

        for fid in FOLDER_IDS:
            if alive.get(fid, False):
                folder_count += 1

        # Counts from versions created this scan
        s_add = s_mod = s_del = 0
        for iv in iv_rows:
            if iv['first_scan_id'] == scan_id:
                if iv['is_added']:
                    s_add += 1
                elif iv['is_deleted']:
                    s_del += 1
                else:
                    s_mod += 1

        new_hash_suspect = sum(1 for hv in hv_rows if hv['first_scan_id'] == scan_id and hv['hash_state'] == SUSPECT)
        new_val_invalid = sum(1 for iv in iv_rows if iv.get('val_scan_id') == scan_id and iv.get('val_state') == INVALID)

        ended = started + random.randint(60, 300)
        scan_rows.append({
            'scan_id': scan_id, 'root_id': 1, 'schedule_id': None,
            'started_at': started, 'ended_at': ended,
            'was_restarted': 0, 'state': 4,
            'is_hash': is_hash, 'hash_all': hash_all, 'is_val': is_val,
            'file_count': file_count, 'folder_count': folder_count,
            'total_size': total_size,
            'new_hash_suspect_count': new_hash_suspect,
            'new_val_invalid_count': new_val_invalid,
            'add_count': s_add, 'modify_count': s_mod, 'delete_count': s_del,
            'val_unknown_count': val_unk, 'val_valid_count': val_valid,
            'val_invalid_count': val_invalid, 'val_no_validator_count': val_noval,
            'hash_unknown_count': hash_unk, 'hash_baseline_count': hash_base,
            'hash_suspect_count': hash_susp, 'error': None,
        })

    # --- Post-processing: reviews ---
    # val_reviewed_at: 5 entries for some invalid versions
    reviews_val = {
        (ID_GLACIER_PHOTO, 1):     scan_started_at(5) + 2 * 86400,
        (ID_HIKING_TRAIL_MAP, 1):  scan_started_at(5) + 3 * 86400,
        (ID_CHASE_FLIGHT, 6):      scan_started_at(8) + 86400,
        (n("Hostel Booking.pdf"), 1): scan_started_at(5) + 86400,
        (n("Ferry Schedule.pdf"), 1): scan_started_at(6) + 2 * 86400,
    }
    # hash_reviewed_at: 3 entries for deep suspect and one moderate suspect
    reviews_hash = {
        (ID_GLACIER_PHOTO, 1):    scan_started_at(10) + 86400,
        (ID_HIKING_TRAIL_MAP, 1): scan_started_at(12) + 86400,
        (ID_SUNSET_PANORAMA, 2):  scan_started_at(15) + 86400,
    }
    for key, ts in reviews_val.items():
        if key in iv_idx:
            iv_rows[iv_idx[key]]['val_reviewed_at'] = ts
    for key, ts in reviews_hash.items():
        if key in iv_idx:
            iv_rows[iv_idx[key]]['hash_reviewed_at'] = ts

    return iv_rows, hv_rows, scan_rows


# ═══════════════════════════════════════════════════════════════
# Schema SQL (from base.rs, schema version 29)
# ═══════════════════════════════════════════════════════════════
SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '29');

CREATE TABLE IF NOT EXISTS roots (
    root_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path TEXT NOT NULL UNIQUE
);
CREATE INDEX IF NOT EXISTS idx_roots_path ON roots (root_path COLLATE natural_path);

CREATE TABLE IF NOT EXISTS scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    schedule_id INTEGER DEFAULT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER DEFAULT NULL,
    was_restarted BOOLEAN NOT NULL DEFAULT 0,
    state INTEGER NOT NULL,
    is_hash BOOLEAN NOT NULL,
    hash_all BOOLEAN NOT NULL,
    is_val BOOLEAN NOT NULL,
    file_count INTEGER DEFAULT NULL,
    folder_count INTEGER DEFAULT NULL,
    total_size INTEGER DEFAULT NULL,
    new_hash_suspect_count INTEGER DEFAULT NULL,
    new_val_invalid_count INTEGER DEFAULT NULL,
    add_count INTEGER DEFAULT NULL,
    modify_count INTEGER DEFAULT NULL,
    delete_count INTEGER DEFAULT NULL,
    val_unknown_count INTEGER DEFAULT NULL,
    val_valid_count INTEGER DEFAULT NULL,
    val_invalid_count INTEGER DEFAULT NULL,
    val_no_validator_count INTEGER DEFAULT NULL,
    hash_unknown_count INTEGER DEFAULT NULL,
    hash_baseline_count INTEGER DEFAULT NULL,
    hash_suspect_count INTEGER DEFAULT NULL,
    error TEXT DEFAULT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id)
);
CREATE INDEX IF NOT EXISTS idx_scans_root ON scans (root_id);

CREATE TABLE IF NOT EXISTS items (
    item_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    item_path TEXT NOT NULL,
    item_name TEXT NOT NULL,
    file_extension TEXT,
    item_type INTEGER NOT NULL,
    has_validator INTEGER NOT NULL DEFAULT 0,
    do_not_validate INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    UNIQUE (root_id, item_path, item_type)
);
CREATE INDEX IF NOT EXISTS idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX IF NOT EXISTS idx_items_root_name ON items (root_id, item_name COLLATE natural_path);
CREATE INDEX IF NOT EXISTS idx_items_root_ext ON items (root_id, file_extension);

CREATE TABLE IF NOT EXISTS item_versions (
    item_id INTEGER NOT NULL,
    item_version INTEGER NOT NULL,
    root_id INTEGER NOT NULL,
    first_scan_id INTEGER NOT NULL,
    last_scan_id INTEGER NOT NULL,
    is_added BOOLEAN NOT NULL DEFAULT 0,
    is_deleted BOOLEAN NOT NULL DEFAULT 0,
    access INTEGER NOT NULL DEFAULT 0,
    mod_date INTEGER,
    size INTEGER,
    add_count INTEGER,
    modify_count INTEGER,
    delete_count INTEGER,
    unchanged_count INTEGER,
    val_scan_id INTEGER,
    val_state INTEGER,
    val_error TEXT,
    val_reviewed_at INTEGER DEFAULT NULL,
    hash_reviewed_at INTEGER DEFAULT NULL,
    PRIMARY KEY (item_id, item_version),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_versions_first_scan ON item_versions (first_scan_id);
CREATE INDEX IF NOT EXISTS idx_versions_root_lastscan ON item_versions (root_id, last_scan_id);
CREATE INDEX IF NOT EXISTS idx_versions_val_scan ON item_versions (val_scan_id, val_state);

CREATE TABLE IF NOT EXISTS hash_versions (
    item_id INTEGER NOT NULL,
    item_version INTEGER NOT NULL,
    first_scan_id INTEGER NOT NULL,
    last_scan_id INTEGER NOT NULL,
    file_hash BLOB NOT NULL,
    hash_state INTEGER NOT NULL,
    PRIMARY KEY (item_id, item_version, first_scan_id),
    FOREIGN KEY (item_id, item_version) REFERENCES item_versions(item_id, item_version),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS idx_hash_versions_first_scan ON hash_versions (first_scan_id, hash_state);

CREATE TABLE IF NOT EXISTS scan_undo_log (
    log_type INTEGER NOT NULL,
    ref_id1 INTEGER NOT NULL,
    ref_id2 INTEGER NOT NULL,
    ref_id3 INTEGER NOT NULL DEFAULT 0,
    old_last_scan_id INTEGER NOT NULL,
    PRIMARY KEY (log_type, ref_id1, ref_id2, ref_id3)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS scan_schedules (
    schedule_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    schedule_name TEXT NOT NULL,
    schedule_type INTEGER NOT NULL CHECK(schedule_type IN (0, 1, 2, 3)),
    time_of_day TEXT,
    days_of_week TEXT,
    day_of_month INTEGER,
    interval_value INTEGER,
    interval_unit INTEGER CHECK(interval_unit IN (0, 1, 2, 3)),
    hash_mode INTEGER NOT NULL CHECK(hash_mode IN (0, 1, 2)),
    is_val BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER DEFAULT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);
CREATE INDEX IF NOT EXISTS idx_scan_schedules_enabled ON scan_schedules(enabled);
CREATE INDEX IF NOT EXISTS idx_scan_schedules_root ON scan_schedules(root_id);
CREATE INDEX IF NOT EXISTS idx_scan_schedules_deleted ON scan_schedules(deleted_at);

CREATE TABLE IF NOT EXISTS tasks (
    task_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_type INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 0,
    root_id INTEGER,
    schedule_id INTEGER,
    run_at INTEGER NOT NULL DEFAULT 0,
    source INTEGER NOT NULL CHECK(source IN (0, 1)),
    task_settings TEXT NOT NULL,
    task_state TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id) ON DELETE SET NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);
CREATE INDEX IF NOT EXISTS idx_tasks_status_source_runat ON tasks(status, source, run_at, task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_schedule ON tasks(schedule_id) WHERE schedule_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_root ON tasks(root_id);
"""


# ═══════════════════════════════════════════════════════════════
# Write database
# ═══════════════════════════════════════════════════════════════
def write_db(iv_rows, hv_rows, scan_rows):
    # Output to tests/fixtures/ relative to the project root
    script_dir = os.path.dirname(os.path.abspath(__file__))
    db_path = os.path.join(script_dir, "fspulse.db")
    db_path = os.path.normpath(db_path)
    os.makedirs(os.path.dirname(db_path), exist_ok=True)
    if os.path.exists(db_path):
        os.remove(db_path)

    conn = sqlite3.connect(db_path)
    conn.create_collation("natural_path", lambda a, b: (a > b) - (a < b))
    conn.executescript(SCHEMA_SQL)

    # Root
    conn.execute("INSERT INTO roots (root_id, root_path) VALUES (1, ?)", (ROOT_PATH,))

    # Items
    for item in ITEMS:
        iid, rel, name, itype, ext, hv, dnv = item
        path = ROOT_PATH + "/" + rel
        conn.execute(
            "INSERT INTO items (item_id, root_id, item_path, item_name, file_extension, item_type, has_validator, do_not_validate) VALUES (?,1,?,?,?,?,?,?)",
            (iid, path, name, ext, itype, hv, dnv),
        )

    # Scans
    for s in scan_rows:
        conn.execute(
            """INSERT INTO scans (scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state,
               is_hash, hash_all, is_val, file_count, folder_count, total_size,
               new_hash_suspect_count, new_val_invalid_count, add_count, modify_count, delete_count,
               val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
               hash_unknown_count, hash_baseline_count, hash_suspect_count, error)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            (s['scan_id'], s['root_id'], s['schedule_id'], s['started_at'], s['ended_at'],
             s['was_restarted'], s['state'], s['is_hash'], s['hash_all'], s['is_val'],
             s['file_count'], s['folder_count'], s['total_size'],
             s['new_hash_suspect_count'], s['new_val_invalid_count'],
             s['add_count'], s['modify_count'], s['delete_count'],
             s['val_unknown_count'], s['val_valid_count'], s['val_invalid_count'], s['val_no_validator_count'],
             s['hash_unknown_count'], s['hash_baseline_count'], s['hash_suspect_count'], s['error']),
        )

    # Item versions
    for iv in iv_rows:
        conn.execute(
            """INSERT INTO item_versions (item_id, item_version, root_id, first_scan_id, last_scan_id,
               is_added, is_deleted, access, mod_date, size,
               add_count, modify_count, delete_count, unchanged_count,
               val_scan_id, val_state, val_error, val_reviewed_at, hash_reviewed_at)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)""",
            (iv['item_id'], iv['item_version'], iv['root_id'],
             iv['first_scan_id'], iv['last_scan_id'],
             iv['is_added'], iv['is_deleted'], iv['access'],
             iv['mod_date'], iv['size'],
             iv['add_count'], iv['modify_count'], iv['delete_count'], iv['unchanged_count'],
             iv['val_scan_id'], iv['val_state'], iv['val_error'],
             iv['val_reviewed_at'], iv['hash_reviewed_at']),
        )

    # Hash versions
    for hv in hv_rows:
        conn.execute(
            """INSERT INTO hash_versions (item_id, item_version, first_scan_id, last_scan_id, file_hash, hash_state)
               VALUES (?,?,?,?,?,?)""",
            (hv['item_id'], hv['item_version'], hv['first_scan_id'], hv['last_scan_id'],
             hv['hash'], hv['hash_state']),
        )

    # Tasks: one per scan
    for s in scan_rows:
        if not s['is_hash']:
            mode = "None"
        elif s['hash_all']:
            mode = "All"
        else:
            mode = "New"
        settings = json.dumps({"hash_mode": mode, "is_val": bool(s['is_val'])})
        state = json.dumps({"scan_id": s['scan_id'], "high_water_mark": 0})
        conn.execute(
            """INSERT INTO tasks (task_type, status, root_id, schedule_id, run_at, source,
               task_settings, task_state, created_at, started_at, completed_at)
               VALUES (0, 2, 1, NULL, 0, 0, ?, ?, ?, ?, ?)""",
            (settings, state, s['started_at'], s['started_at'], s['ended_at']),
        )

    conn.commit()
    conn.close()
    print(f"Generated {db_path}")
    return db_path


# ═══════════════════════════════════════════════════════════════
# Verification
# ═══════════════════════════════════════════════════════════════
def verify(db_path):
    conn = sqlite3.connect(db_path)
    conn.create_collation("natural_path", lambda a, b: (a > b) - (a < b))

    checks = []

    # 1. Folders exist
    folders = conn.execute("SELECT COUNT(*) FROM items WHERE item_type = 1").fetchone()[0]
    checks.append(("Folders exist", folders > 0, f"{folders} folders"))

    # 2. At least 100 files
    files = conn.execute("SELECT COUNT(*) FROM items WHERE item_type = 0").fetchone()[0]
    checks.append(("At least 100 files", files >= 100, f"{files} files"))

    # 3. Deleted versions exist
    dels = conn.execute("SELECT COUNT(*) FROM item_versions WHERE is_deleted = 1").fetchone()[0]
    checks.append(("Deleted versions exist", dels > 0, f"{dels} deleted"))

    # 4. val_state uses NULL not 0
    zeroes = conn.execute("SELECT COUNT(*) FROM item_versions WHERE val_state = 0").fetchone()[0]
    checks.append(("val_state NULL not 0", zeroes == 0, f"{zeroes} zeroes"))

    # 5. First hash per version is always Baseline
    bad_first = conn.execute("""
        SELECT COUNT(*) FROM hash_versions hv
        WHERE hv.hash_state != 1
          AND hv.first_scan_id = (
            SELECT MIN(first_scan_id) FROM hash_versions
            WHERE item_id = hv.item_id AND item_version = hv.item_version)
    """).fetchone()[0]
    checks.append(("First hash = Baseline", bad_first == 0, f"{bad_first} violations"))

    # 6. No Baseline after Suspect
    bad_seq = conn.execute("""
        SELECT COUNT(*) FROM hash_versions a
        JOIN hash_versions b ON a.item_id = b.item_id AND a.item_version = b.item_version
          AND b.first_scan_id > a.first_scan_id
          AND NOT EXISTS (
            SELECT 1 FROM hash_versions c
            WHERE c.item_id = a.item_id AND c.item_version = a.item_version
              AND c.first_scan_id > a.first_scan_id AND c.first_scan_id < b.first_scan_id)
        WHERE a.hash_state = 2 AND b.hash_state = 1
    """).fetchone()[0]
    checks.append(("No Baseline after Suspect", bad_seq == 0, f"{bad_seq} violations"))

    # 7. Deep suspect chains (21+)
    deep = conn.execute("""
        SELECT COUNT(*) FROM (
            SELECT item_id, item_version, SUM(CASE WHEN hash_state=2 THEN 1 ELSE 0 END) as sc
            FROM hash_versions GROUP BY item_id, item_version HAVING sc >= 21)
    """).fetchone()[0]
    checks.append(("Deep suspect chains (21+)", deep >= 2, f"{deep} versions"))

    # 8. High churn item with 40+ versions
    max_ver = conn.execute("SELECT MAX(c) FROM (SELECT COUNT(*) as c FROM item_versions GROUP BY item_id)").fetchone()[0]
    checks.append(("High churn (40+ versions)", max_ver >= 40, f"max={max_ver}"))

    # 9. Browse query returns results for root
    last_scan = 50
    browse = conn.execute("""
        SELECT COUNT(*) FROM items i
        JOIN item_versions iv ON iv.item_id = i.item_id
        WHERE i.root_id = 1
          AND iv.first_scan_id = (SELECT MAX(first_scan_id) FROM item_versions WHERE item_id = i.item_id AND first_scan_id <= ?)
          AND (iv.is_deleted = 0 OR iv.first_scan_id = ?)
          AND i.item_path > ? AND i.item_path < ?
          AND i.item_path != ?
          AND SUBSTR(i.item_path, LENGTH(?) + 1) NOT LIKE '%/%'
    """, (last_scan, last_scan, ROOT_PATH + '/', ROOT_PATH + '0', ROOT_PATH, ROOT_PATH + '/')).fetchone()[0]
    checks.append(("Browse root has children", browse > 0, f"{browse} immediate children"))

    # 10. Rehydrations exist
    rehydr = conn.execute("""
        SELECT COUNT(*) FROM item_versions WHERE is_added = 1 AND item_version > 1
    """).fetchone()[0]
    checks.append(("Rehydrations exist", rehydr > 0, f"{rehydr} rehydrations"))

    # 11. Scan counts match
    ok = True
    for scan_id in range(1, 51):
        row = conn.execute("SELECT add_count, modify_count, delete_count FROM scans WHERE scan_id = ?", (scan_id,)).fetchone()
        actual_add = conn.execute("SELECT COUNT(*) FROM item_versions WHERE first_scan_id = ? AND is_added = 1", (scan_id,)).fetchone()[0]
        actual_del = conn.execute("SELECT COUNT(*) FROM item_versions WHERE first_scan_id = ? AND is_deleted = 1", (scan_id,)).fetchone()[0]
        actual_mod = conn.execute("SELECT COUNT(*) FROM item_versions WHERE first_scan_id = ? AND is_added = 0 AND is_deleted = 0", (scan_id,)).fetchone()[0]
        if row[0] != actual_add or row[1] != actual_mod or row[2] != actual_del:
            ok = False
            break
    checks.append(("Scan counts consistent", ok, f"all 50 scans"))

    # 12. Tasks table has entries
    task_count = conn.execute("SELECT COUNT(*) FROM tasks").fetchone()[0]
    checks.append(("Tasks table populated", task_count == 50, f"{task_count} tasks"))

    # 13. 30+ files with integrity issues (val_state=2 or hash_state=2 on latest hash)
    integrity_issues = conn.execute("""
        SELECT COUNT(DISTINCT i.item_id) FROM items i
        JOIN item_versions iv ON iv.item_id = i.item_id
        LEFT JOIN hash_versions hv ON hv.item_id = iv.item_id
            AND hv.item_version = iv.item_version
            AND hv.first_scan_id = (
                SELECT MAX(first_scan_id) FROM hash_versions
                WHERE item_id = iv.item_id AND item_version = iv.item_version)
        WHERE iv.val_state = 2 OR hv.hash_state = 2
    """).fetchone()[0]
    checks.append(("30+ integrity issues", integrity_issues >= 30, f"{integrity_issues} items"))

    # 14. Invalid validations count
    invalid_count = conn.execute("SELECT COUNT(*) FROM item_versions WHERE val_state = 2").fetchone()[0]
    checks.append(("30+ invalid validations", invalid_count >= 30, f"{invalid_count} invalids"))

    # 15. Cross-version suspects (items with suspects on 2+ different versions)
    cross_ver = conn.execute("""
        SELECT COUNT(*) FROM (
            SELECT item_id, COUNT(DISTINCT item_version) as ver_count
            FROM hash_versions WHERE hash_state = 2
            GROUP BY item_id HAVING ver_count >= 2)
    """).fetchone()[0]
    checks.append(("Cross-version suspects", cross_ver >= 3, f"{cross_ver} items"))

    conn.close()

    print("\nVerification:")
    all_ok = True
    for name, passed, detail in checks:
        status = "PASS" if passed else "FAIL"
        if not passed:
            all_ok = False
        print(f"  [{status}] {name}: {detail}")

    return all_ok


if __name__ == "__main__":
    iv_rows, hv_rows, scan_rows = simulate()
    db_path = write_db(iv_rows, hv_rows, scan_rows)
    ok = verify(db_path)
    if not ok:
        print("\nSome checks FAILED!")
        exit(1)
    else:
        print("\nAll checks passed.")
