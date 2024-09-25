pub(super) static VERSION: Option<&str> = Some(r"2024b");

pub(super) static TZNAME_TO_OFFSET: &[(&str, core::ops::Range<usize>)] = &[
    (r"Africa/Abidjan", 0..130),
    (r"Africa/Accra", 130..260),
    (r"Africa/Addis_Ababa", 260..451),
    (r"Africa/Algiers", 451..921),
    (r"Africa/Asmara", 921..1112),
    (r"Africa/Asmera", 1112..1303),
    (r"Africa/Bamako", 1303..1433),
    (r"Africa/Bangui", 1433..1613),
    (r"Africa/Banjul", 1613..1743),
    (r"Africa/Bissau", 1743..1892),
    (r"Africa/Blantyre", 1892..2023),
    (r"Africa/Brazzaville", 2023..2203),
    (r"Africa/Bujumbura", 2203..2334),
    (r"Africa/Cairo", 2334..3643),
    (r"Africa/Casablanca", 3643..5562),
    (r"Africa/Ceuta", 5562..6124),
    (r"Africa/Conakry", 6124..6254),
    (r"Africa/Dakar", 6254..6384),
    (r"Africa/Dar_es_Salaam", 6384..6575),
    (r"Africa/Djibouti", 6575..6766),
    (r"Africa/Douala", 6766..6946),
    (r"Africa/El_Aaiun", 6946..8776),
    (r"Africa/Freetown", 8776..8906),
    (r"Africa/Gaborone", 8906..9037),
    (r"Africa/Harare", 9037..9168),
    (r"Africa/Johannesburg", 9168..9358),
    (r"Africa/Juba", 9358..9816),
    (r"Africa/Kampala", 9816..10007),
    (r"Africa/Khartoum", 10007..10465),
    (r"Africa/Kigali", 10465..10596),
    (r"Africa/Kinshasa", 10596..10776),
    (r"Africa/Lagos", 10776..10956),
    (r"Africa/Libreville", 10956..11136),
    (r"Africa/Lome", 11136..11266),
    (r"Africa/Luanda", 11266..11446),
    (r"Africa/Lubumbashi", 11446..11577),
    (r"Africa/Lusaka", 11577..11708),
    (r"Africa/Malabo", 11708..11888),
    (r"Africa/Maputo", 11888..12019),
    (r"Africa/Maseru", 12019..12209),
    (r"Africa/Mbabane", 12209..12399),
    (r"Africa/Mogadishu", 12399..12590),
    (r"Africa/Monrovia", 12590..12754),
    (r"Africa/Nairobi", 12754..12945),
    (r"Africa/Ndjamena", 12945..13105),
    (r"Africa/Niamey", 13105..13285),
    (r"Africa/Nouakchott", 13285..13415),
    (r"Africa/Ouagadougou", 13415..13545),
    (r"Africa/Porto-Novo", 13545..13725),
    (r"Africa/Sao_Tome", 13725..13898),
    (r"Africa/Timbuktu", 13898..14028),
    (r"Africa/Tripoli", 14028..14459),
    (r"Africa/Tunis", 14459..14908),
    (r"Africa/Windhoek", 14908..15546),
    (r"America/Adak", 15546..16515),
    (r"America/Anchorage", 16515..17492),
    (r"America/Anguilla", 17492..17669),
    (r"America/Antigua", 17669..17846),
    (r"America/Araguaina", 17846..18438),
    (r"America/Argentina/Buenos_Aires", 18438..19146),
    (r"America/Argentina/Catamarca", 19146..19854),
    (r"America/Argentina/ComodRivadavia", 19854..20562),
    (r"America/Argentina/Cordoba", 20562..21270),
    (r"America/Argentina/Jujuy", 21270..21960),
    (r"America/Argentina/La_Rioja", 21960..22677),
    (r"America/Argentina/Mendoza", 22677..23385),
    (r"America/Argentina/Rio_Gallegos", 23385..24093),
    (r"America/Argentina/Salta", 24093..24783),
    (r"America/Argentina/San_Juan", 24783..25500),
    (r"America/Argentina/San_Luis", 25500..26217),
    (r"America/Argentina/Tucuman", 26217..26943),
    (r"America/Argentina/Ushuaia", 26943..27651),
    (r"America/Aruba", 27651..27828),
    (r"America/Asuncion", 27828..28712),
    (r"America/Atikokan", 28712..28861),
    (r"America/Atka", 28861..29830),
    (r"America/Bahia", 29830..30512),
    (r"America/Bahia_Banderas", 30512..31212),
    (r"America/Barbados", 31212..31490),
    (r"America/Belem", 31490..31884),
    (r"America/Belize", 31884..32929),
    (r"America/Blanc-Sablon", 32929..33106),
    (r"America/Boa_Vista", 33106..33536),
    (r"America/Bogota", 33536..33715),
    (r"America/Boise", 33715..34714),
    (r"America/Buenos_Aires", 34714..35422),
    (r"America/Cambridge_Bay", 35422..36305),
    (r"America/Campo_Grande", 36305..37257),
    (r"America/Cancun", 37257..37795),
    (r"America/Caracas", 37795..37985),
    (r"America/Catamarca", 37985..38693),
    (r"America/Cayenne", 38693..38844),
    (r"America/Cayman", 38844..38993),
    (r"America/Chicago", 38993..40747),
    (r"America/Chihuahua", 40747..41438),
    (r"America/Ciudad_Juarez", 41438..42156),
    (r"America/Coral_Harbour", 42156..42305),
    (r"America/Cordoba", 42305..43013),
    (r"America/Costa_Rica", 43013..43245),
    (r"America/Creston", 43245..43485),
    (r"America/Cuiaba", 43485..44419),
    (r"America/Curacao", 44419..44596),
    (r"America/Danmarkshavn", 44596..45043),
    (r"America/Dawson", 45043..46072),
    (r"America/Dawson_Creek", 46072..46755),
    (r"America/Denver", 46755..47797),
    (r"America/Detroit", 47797..48696),
    (r"America/Dominica", 48696..48873),
    (r"America/Edmonton", 48873..49843),
    (r"America/Eirunepe", 49843..50279),
    (r"America/El_Salvador", 50279..50455),
    (r"America/Ensenada", 50455..51534),
    (r"America/Fort_Nelson", 51534..52982),
    (r"America/Fort_Wayne", 52982..53513),
    (r"America/Fortaleza", 53513..53997),
    (r"America/Glace_Bay", 53997..54877),
    (r"America/Godthab", 54877..55842),
    (r"America/Goose_Bay", 55842..57422),
    (r"America/Grand_Turk", 57422..58275),
    (r"America/Grenada", 58275..58452),
    (r"America/Guadeloupe", 58452..58629),
    (r"America/Guatemala", 58629..58841),
    (r"America/Guayaquil", 58841..59020),
    (r"America/Guyana", 59020..59201),
    (r"America/Halifax", 59201..60873),
    (r"America/Havana", 60873..61990),
    (r"America/Hermosillo", 61990..62248),
    (r"America/Indiana/Indianapolis", 62248..62779),
    (r"America/Indiana/Knox", 62779..63795),
    (r"America/Indiana/Marengo", 63795..64362),
    (r"America/Indiana/Petersburg", 64362..65045),
    (r"America/Indiana/Tell_City", 65045..65567),
    (r"America/Indiana/Vevay", 65567..65936),
    (r"America/Indiana/Vincennes", 65936..66494),
    (r"America/Indiana/Winamac", 66494..67097),
    (r"America/Indianapolis", 67097..67628),
    (r"America/Inuvik", 67628..68445),
    (r"America/Iqaluit", 68445..69300),
    (r"America/Jamaica", 69300..69639),
    (r"America/Jujuy", 69639..70329),
    (r"America/Juneau", 70329..71295),
    (r"America/Kentucky/Louisville", 71295..72537),
    (r"America/Kentucky/Monticello", 72537..73509),
    (r"America/Knox_IN", 73509..74525),
    (r"America/Kralendijk", 74525..74702),
    (r"America/La_Paz", 74702..74872),
    (r"America/Lima", 74872..75155),
    (r"America/Los_Angeles", 75155..76449),
    (r"America/Louisville", 76449..77691),
    (r"America/Lower_Princes", 77691..77868),
    (r"America/Maceio", 77868..78370),
    (r"America/Managua", 78370..78665),
    (r"America/Manaus", 78665..79077),
    (r"America/Marigot", 79077..79254),
    (r"America/Martinique", 79254..79432),
    (r"America/Matamoros", 79432..79869),
    (r"America/Mazatlan", 79869..80559),
    (r"America/Mendoza", 80559..81267),
    (r"America/Menominee", 81267..82184),
    (r"America/Merida", 82184..82838),
    (r"America/Metlakatla", 82838..83424),
    (r"America/Mexico_City", 83424..84197),
    (r"America/Miquelon", 84197..84747),
    (r"America/Moncton", 84747..86240),
    (r"America/Monterrey", 86240..86949),
    (r"America/Montevideo", 86949..87918),
    (r"America/Montreal", 87918..89635),
    (r"America/Montserrat", 89635..89812),
    (r"America/Nassau", 89812..91529),
    (r"America/New_York", 91529..93273),
    (r"America/Nipigon", 93273..94990),
    (r"America/Nome", 94990..95965),
    (r"America/Noronha", 95965..96449),
    (r"America/North_Dakota/Beulah", 96449..97492),
    (r"America/North_Dakota/Center", 97492..98482),
    (r"America/North_Dakota/New_Salem", 98482..99472),
    (r"America/Nuuk", 99472..100437),
    (r"America/Ojinaga", 100437..101155),
    (r"America/Panama", 101155..101304),
    (r"America/Pangnirtung", 101304..102159),
    (r"America/Paramaribo", 102159..102346),
    (r"America/Phoenix", 102346..102586),
    (r"America/Port-au-Prince", 102586..103151),
    (r"America/Port_of_Spain", 103151..103328),
    (r"America/Porto_Acre", 103328..103746),
    (r"America/Porto_Velho", 103746..104140),
    (r"America/Puerto_Rico", 104140..104317),
    (r"America/Punta_Arenas", 104317..105535),
    (r"America/Rainy_River", 105535..106829),
    (r"America/Rankin_Inlet", 106829..107636),
    (r"America/Recife", 107636..108120),
    (r"America/Regina", 108120..108758),
    (r"America/Resolute", 108758..109565),
    (r"America/Rio_Branco", 109565..109983),
    (r"America/Rosario", 109983..110691),
    (r"America/Santa_Isabel", 110691..111770),
    (r"America/Santarem", 111770..112179),
    (r"America/Santiago", 112179..113533),
    (r"America/Santo_Domingo", 113533..113850),
    (r"America/Sao_Paulo", 113850..114802),
    (r"America/Scoresbysund", 114802..115786),
    (r"America/Shiprock", 115786..116828),
    (r"America/Sitka", 116828..117784),
    (r"America/St_Barthelemy", 117784..117961),
    (r"America/St_Johns", 117961..119839),
    (r"America/St_Kitts", 119839..120016),
    (r"America/St_Lucia", 120016..120193),
    (r"America/St_Thomas", 120193..120370),
    (r"America/St_Vincent", 120370..120547),
    (r"America/Swift_Current", 120547..120915),
    (r"America/Tegucigalpa", 120915..121109),
    (r"America/Thule", 121109..121564),
    (r"America/Thunder_Bay", 121564..123281),
    (r"America/Tijuana", 123281..124360),
    (r"America/Toronto", 124360..126077),
    (r"America/Tortola", 126077..126254),
    (r"America/Vancouver", 126254..127584),
    (r"America/Virgin", 127584..127761),
    (r"America/Whitehorse", 127761..128790),
    (r"America/Winnipeg", 128790..130084),
    (r"America/Yakutat", 130084..131030),
    (r"America/Yellowknife", 131030..132000),
    (r"Antarctica/Casey", 132000..132287),
    (r"Antarctica/Davis", 132287..132484),
    (r"Antarctica/DumontDUrville", 132484..132638),
    (r"Antarctica/Macquarie", 132638..133614),
    (r"Antarctica/Mawson", 133614..133766),
    (r"Antarctica/McMurdo", 133766..134809),
    (r"Antarctica/Palmer", 134809..135696),
    (r"Antarctica/Rothera", 135696..135828),
    (r"Antarctica/South_Pole", 135828..136871),
    (r"Antarctica/Syowa", 136871..137004),
    (r"Antarctica/Troll", 137004..137162),
    (r"Antarctica/Vostok", 137162..137332),
    (r"Arctic/Longyearbyen", 137332..138037),
    (r"Asia/Aden", 138037..138170),
    (r"Asia/Almaty", 138170..138788),
    (r"Asia/Amman", 138788..139716),
    (r"Asia/Anadyr", 139716..140459),
    (r"Asia/Aqtau", 140459..141065),
    (r"Asia/Aqtobe", 141065..141680),
    (r"Asia/Ashgabat", 141680..142055),
    (r"Asia/Ashkhabad", 142055..142430),
    (r"Asia/Atyrau", 142430..143046),
    (r"Asia/Baghdad", 143046..143676),
    (r"Asia/Bahrain", 143676..143828),
    (r"Asia/Baku", 143828..144572),
    (r"Asia/Bangkok", 144572..144724),
    (r"Asia/Barnaul", 144724..145477),
    (r"Asia/Beirut", 145477..146209),
    (r"Asia/Bishkek", 146209..146827),
    (r"Asia/Brunei", 146827..147147),
    (r"Asia/Calcutta", 147147..147367),
    (r"Asia/Chita", 147367..148117),
    (r"Asia/Choibalsan", 148117..148711),
    (r"Asia/Chongqing", 148711..149104),
    (r"Asia/Chungking", 149104..149497),
    (r"Asia/Colombo", 149497..149744),
    (r"Asia/Dacca", 149744..149975),
    (r"Asia/Damascus", 149975..151209),
    (r"Asia/Dhaka", 151209..151440),
    (r"Asia/Dili", 151440..151610),
    (r"Asia/Dubai", 151610..151743),
    (r"Asia/Dushanbe", 151743..152109),
    (r"Asia/Famagusta", 152109..153049),
    (r"Asia/Gaza", 153049..155999),
    (r"Asia/Harbin", 155999..156392),
    (r"Asia/Hebron", 156392..159360),
    (r"Asia/Ho_Chi_Minh", 159360..159596),
    (r"Asia/Hong_Kong", 159596..160371),
    (r"Asia/Hovd", 160371..160965),
    (r"Asia/Irkutsk", 160965..161725),
    (r"Asia/Istanbul", 161725..162925),
    (r"Asia/Jakarta", 162925..163173),
    (r"Asia/Jayapura", 163173..163344),
    (r"Asia/Jerusalem", 163344..164418),
    (r"Asia/Kabul", 164418..164577),
    (r"Asia/Kamchatka", 164577..165304),
    (r"Asia/Karachi", 165304..165570),
    (r"Asia/Kashgar", 165570..165703),
    (r"Asia/Kathmandu", 165703..165864),
    (r"Asia/Katmandu", 165864..166025),
    (r"Asia/Khandyga", 166025..166800),
    (r"Asia/Kolkata", 166800..167020),
    (r"Asia/Krasnoyarsk", 167020..167761),
    (r"Asia/Kuala_Lumpur", 167761..168017),
    (r"Asia/Kuching", 168017..168337),
    (r"Asia/Kuwait", 168337..168470),
    (r"Asia/Macao", 168470..169261),
    (r"Asia/Macau", 169261..170052),
    (r"Asia/Magadan", 170052..170803),
    (r"Asia/Makassar", 170803..170993),
    (r"Asia/Manila", 170993..171231),
    (r"Asia/Muscat", 171231..171364),
    (r"Asia/Nicosia", 171364..171961),
    (r"Asia/Novokuznetsk", 171961..172687),
    (r"Asia/Novosibirsk", 172687..173440),
    (r"Asia/Omsk", 173440..174181),
    (r"Asia/Oral", 174181..174806),
    (r"Asia/Phnom_Penh", 174806..174958),
    (r"Asia/Pontianak", 174958..175205),
    (r"Asia/Pyongyang", 175205..175388),
    (r"Asia/Qatar", 175388..175540),
    (r"Asia/Qostanay", 175540..176164),
    (r"Asia/Qyzylorda", 176164..176788),
    (r"Asia/Rangoon", 176788..176975),
    (r"Asia/Riyadh", 176975..177108),
    (r"Asia/Saigon", 177108..177344),
    (r"Asia/Sakhalin", 177344..178099),
    (r"Asia/Samarkand", 178099..178465),
    (r"Asia/Seoul", 178465..178880),
    (r"Asia/Shanghai", 178880..179273),
    (r"Asia/Singapore", 179273..179529),
    (r"Asia/Srednekolymsk", 179529..180271),
    (r"Asia/Taipei", 180271..180782),
    (r"Asia/Tashkent", 180782..181148),
    (r"Asia/Tbilisi", 181148..181777),
    (r"Asia/Tehran", 181777..182589),
    (r"Asia/Tel_Aviv", 182589..183663),
    (r"Asia/Thimbu", 183663..183817),
    (r"Asia/Thimphu", 183817..183971),
    (r"Asia/Tokyo", 183971..184184),
    (r"Asia/Tomsk", 184184..184937),
    (r"Asia/Ujung_Pandang", 184937..185127),
    (r"Asia/Ulaanbaatar", 185127..185721),
    (r"Asia/Ulan_Bator", 185721..186315),
    (r"Asia/Urumqi", 186315..186448),
    (r"Asia/Ust-Nera", 186448..187219),
    (r"Asia/Vientiane", 187219..187371),
    (r"Asia/Vladivostok", 187371..188113),
    (r"Asia/Yakutsk", 188113..188854),
    (r"Asia/Yangon", 188854..189041),
    (r"Asia/Yekaterinburg", 189041..189801),
    (r"Asia/Yerevan", 189801..190509),
    (r"Atlantic/Azores", 190509..191910),
    (r"Atlantic/Bermuda", 191910..192934),
    (r"Atlantic/Canary", 192934..193412),
    (r"Atlantic/Cape_Verde", 193412..193587),
    (r"Atlantic/Faeroe", 193587..194028),
    (r"Atlantic/Faroe", 194028..194469),
    (r"Atlantic/Jan_Mayen", 194469..195174),
    (r"Atlantic/Madeira", 195174..196546),
    (r"Atlantic/Reykjavik", 196546..196676),
    (r"Atlantic/South_Georgia", 196676..196808),
    (r"Atlantic/St_Helena", 196808..196938),
    (r"Atlantic/Stanley", 196938..197727),
    (r"Australia/ACT", 197727..198631),
    (r"Australia/Adelaide", 198631..199552),
    (r"Australia/Brisbane", 199552..199841),
    (r"Australia/Broken_Hill", 199841..200782),
    (r"Australia/Canberra", 200782..201686),
    (r"Australia/Currie", 201686..202689),
    (r"Australia/Darwin", 202689..202923),
    (r"Australia/Eucla", 202923..203237),
    (r"Australia/Hobart", 203237..204240),
    (r"Australia/LHI", 204240..204932),
    (r"Australia/Lindeman", 204932..205257),
    (r"Australia/Lord_Howe", 205257..205949),
    (r"Australia/Melbourne", 205949..206853),
    (r"Australia/North", 207757..207991),
    (r"Australia/NSW", 206853..207757),
    (r"Australia/Perth", 207991..208297),
    (r"Australia/Queensland", 208297..208586),
    (r"Australia/South", 208586..209507),
    (r"Australia/Sydney", 209507..210411),
    (r"Australia/Tasmania", 210411..211414),
    (r"Australia/Victoria", 211414..212318),
    (r"Australia/West", 212318..212624),
    (r"Australia/Yancowinna", 212624..213565),
    (r"Brazil/Acre", 213565..213983),
    (r"Brazil/DeNoronha", 213983..214467),
    (r"Brazil/East", 214467..215419),
    (r"Brazil/West", 215419..215831),
    (r"Canada/Atlantic", 218688..220360),
    (r"Canada/Central", 220360..221654),
    (r"Canada/Eastern", 221654..223371),
    (r"Canada/Mountain", 223371..224341),
    (r"Canada/Newfoundland", 224341..226219),
    (r"Canada/Pacific", 226219..227549),
    (r"Canada/Saskatchewan", 227549..228187),
    (r"Canada/Yukon", 228187..229216),
    (r"CET", 215831..216934),
    (r"Chile/Continental", 229216..230570),
    (r"Chile/EasterIsland", 230570..231744),
    (r"CST6CDT", 216934..218688),
    (r"Cuba", 231744..232861),
    (r"EET", 232861..233543),
    (r"Egypt", 235436..236745),
    (r"Eire", 236745..238241),
    (r"EST", 233543..233692),
    (r"EST5EDT", 233692..235436),
    (r"Etc/GMT", 238241..238352),
    (r"Etc/GMT+0", 238352..238463),
    (r"Etc/GMT+1", 238463..238576),
    (r"Etc/GMT+10", 238576..238690),
    (r"Etc/GMT+11", 238690..238804),
    (r"Etc/GMT+12", 238804..238918),
    (r"Etc/GMT+2", 238918..239031),
    (r"Etc/GMT+3", 239031..239144),
    (r"Etc/GMT+4", 239144..239257),
    (r"Etc/GMT+5", 239257..239370),
    (r"Etc/GMT+6", 239370..239483),
    (r"Etc/GMT+7", 239483..239596),
    (r"Etc/GMT+8", 239596..239709),
    (r"Etc/GMT+9", 239709..239822),
    (r"Etc/GMT-0", 239822..239933),
    (r"Etc/GMT-1", 239933..240047),
    (r"Etc/GMT-10", 240047..240162),
    (r"Etc/GMT-11", 240162..240277),
    (r"Etc/GMT-12", 240277..240392),
    (r"Etc/GMT-13", 240392..240507),
    (r"Etc/GMT-14", 240507..240622),
    (r"Etc/GMT-2", 240622..240736),
    (r"Etc/GMT-3", 240736..240850),
    (r"Etc/GMT-4", 240850..240964),
    (r"Etc/GMT-5", 240964..241078),
    (r"Etc/GMT-6", 241078..241192),
    (r"Etc/GMT-7", 241192..241306),
    (r"Etc/GMT-8", 241306..241420),
    (r"Etc/GMT-9", 241420..241534),
    (r"Etc/GMT0", 241534..241645),
    (r"Etc/Greenwich", 241645..241756),
    (r"Etc/UCT", 241756..241867),
    (r"Etc/Universal", 241978..242089),
    (r"Etc/UTC", 241867..241978),
    (r"Etc/Zulu", 242089..242200),
    (r"Europe/Amsterdam", 242200..243303),
    (r"Europe/Andorra", 243303..243692),
    (r"Europe/Astrakhan", 243692..244418),
    (r"Europe/Athens", 244418..245100),
    (r"Europe/Belfast", 245100..246699),
    (r"Europe/Belgrade", 246699..247177),
    (r"Europe/Berlin", 247177..247882),
    (r"Europe/Bratislava", 247882..248605),
    (r"Europe/Brussels", 248605..249708),
    (r"Europe/Bucharest", 249708..250369),
    (r"Europe/Budapest", 250369..251135),
    (r"Europe/Busingen", 251135..251632),
    (r"Europe/Chisinau", 251632..252387),
    (r"Europe/Copenhagen", 252387..253092),
    (r"Europe/Dublin", 253092..254588),
    (r"Europe/Gibraltar", 254588..255808),
    (r"Europe/Guernsey", 255808..257407),
    (r"Europe/Helsinki", 257407..257888),
    (r"Europe/Isle_of_Man", 257888..259487),
    (r"Europe/Istanbul", 259487..260687),
    (r"Europe/Jersey", 260687..262286),
    (r"Europe/Kaliningrad", 262286..263190),
    (r"Europe/Kiev", 263190..263748),
    (r"Europe/Kirov", 263748..264483),
    (r"Europe/Kyiv", 264483..265041),
    (r"Europe/Lisbon", 265041..266504),
    (r"Europe/Ljubljana", 266504..266982),
    (r"Europe/London", 266982..268581),
    (r"Europe/Luxembourg", 268581..269684),
    (r"Europe/Madrid", 269684..270581),
    (r"Europe/Malta", 270581..271509),
    (r"Europe/Mariehamn", 271509..271990),
    (r"Europe/Minsk", 271990..272798),
    (r"Europe/Monaco", 272798..273903),
    (r"Europe/Moscow", 273903..274811),
    (r"Europe/Nicosia", 274811..275408),
    (r"Europe/Oslo", 275408..276113),
    (r"Europe/Paris", 276113..277218),
    (r"Europe/Podgorica", 277218..277696),
    (r"Europe/Prague", 277696..278419),
    (r"Europe/Riga", 278419..279113),
    (r"Europe/Rome", 279113..280060),
    (r"Europe/Samara", 280060..280792),
    (r"Europe/San_Marino", 280792..281739),
    (r"Europe/Sarajevo", 281739..282217),
    (r"Europe/Saratov", 282217..282943),
    (r"Europe/Simferopol", 282943..283808),
    (r"Europe/Skopje", 283808..284286),
    (r"Europe/Sofia", 284286..284878),
    (r"Europe/Stockholm", 284878..285583),
    (r"Europe/Tallinn", 285583..286258),
    (r"Europe/Tirane", 286258..286862),
    (r"Europe/Tiraspol", 286862..287617),
    (r"Europe/Ulyanovsk", 287617..288377),
    (r"Europe/Uzhgorod", 288377..288935),
    (r"Europe/Vaduz", 288935..289432),
    (r"Europe/Vatican", 289432..290379),
    (r"Europe/Vienna", 290379..291037),
    (r"Europe/Vilnius", 291037..291713),
    (r"Europe/Volgograd", 291713..292466),
    (r"Europe/Warsaw", 292466..293389),
    (r"Europe/Zagreb", 293389..293867),
    (r"Europe/Zaporozhye", 293867..294425),
    (r"Europe/Zurich", 294425..294922),
    (r"Factory", 294922..295035),
    (r"GB", 295035..296634),
    (r"GB-Eire", 296634..298233),
    (r"GMT", 298233..298344),
    (r"GMT+0", 298344..298455),
    (r"GMT-0", 298455..298566),
    (r"GMT0", 298566..298677),
    (r"Greenwich", 298677..298788),
    (r"Hongkong", 299009..299784),
    (r"HST", 298788..299009),
    (r"Iceland", 299784..299914),
    (r"Indian/Antananarivo", 299914..300105),
    (r"Indian/Chagos", 300105..300257),
    (r"Indian/Christmas", 300257..300409),
    (r"Indian/Cocos", 300409..300596),
    (r"Indian/Comoro", 300596..300787),
    (r"Indian/Kerguelen", 300787..300939),
    (r"Indian/Mahe", 300939..301072),
    (r"Indian/Maldives", 301072..301224),
    (r"Indian/Mauritius", 301224..301403),
    (r"Indian/Mayotte", 301403..301594),
    (r"Indian/Reunion", 301594..301727),
    (r"Iran", 301727..302539),
    (r"Israel", 302539..303613),
    (r"Jamaica", 303613..303952),
    (r"Japan", 303952..304165),
    (r"Kwajalein", 304165..304384),
    (r"Libya", 304384..304815),
    (r"MET", 304815..305918),
    (r"Mexico/BajaNorte", 307200..308279),
    (r"Mexico/BajaSur", 308279..308969),
    (r"Mexico/General", 308969..309742),
    (r"MST", 305918..306158),
    (r"MST7MDT", 306158..307200),
    (r"Navajo", 311593..312635),
    (r"NZ", 309742..310785),
    (r"NZ-CHAT", 310785..311593),
    (r"Pacific/Apia", 314322..314729),
    (r"Pacific/Auckland", 314729..315772),
    (r"Pacific/Bougainville", 315772..315973),
    (r"Pacific/Chatham", 315973..316781),
    (r"Pacific/Chuuk", 316781..316935),
    (r"Pacific/Easter", 316935..318109),
    (r"Pacific/Efate", 318109..318451),
    (r"Pacific/Enderbury", 318451..318623),
    (r"Pacific/Fakaofo", 318623..318776),
    (r"Pacific/Fiji", 318776..319172),
    (r"Pacific/Funafuti", 319172..319306),
    (r"Pacific/Galapagos", 319306..319481),
    (r"Pacific/Gambier", 319481..319613),
    (r"Pacific/Guadalcanal", 319613..319747),
    (r"Pacific/Guam", 319747..320097),
    (r"Pacific/Honolulu", 320097..320318),
    (r"Pacific/Johnston", 320318..320539),
    (r"Pacific/Kanton", 320539..320711),
    (r"Pacific/Kiritimati", 320711..320885),
    (r"Pacific/Kosrae", 320885..321127),
    (r"Pacific/Kwajalein", 321127..321346),
    (r"Pacific/Majuro", 321346..321480),
    (r"Pacific/Marquesas", 321480..321619),
    (r"Pacific/Midway", 321619..321765),
    (r"Pacific/Nauru", 321765..321948),
    (r"Pacific/Niue", 321948..322102),
    (r"Pacific/Norfolk", 322102..322339),
    (r"Pacific/Noumea", 322339..322537),
    (r"Pacific/Pago_Pago", 322537..322683),
    (r"Pacific/Palau", 322683..322831),
    (r"Pacific/Pitcairn", 322831..322984),
    (r"Pacific/Pohnpei", 322984..323118),
    (r"Pacific/Ponape", 323118..323252),
    (r"Pacific/Port_Moresby", 323252..323406),
    (r"Pacific/Rarotonga", 323406..323812),
    (r"Pacific/Saipan", 323812..324162),
    (r"Pacific/Samoa", 324162..324308),
    (r"Pacific/Tahiti", 324308..324441),
    (r"Pacific/Tarawa", 324441..324575),
    (r"Pacific/Tongatapu", 324575..324812),
    (r"Pacific/Truk", 324812..324966),
    (r"Pacific/Wake", 324966..325100),
    (r"Pacific/Wallis", 325100..325234),
    (r"Pacific/Yap", 325234..325388),
    (r"Poland", 325388..326311),
    (r"Portugal", 326311..327774),
    (r"PRC", 312635..313028),
    (r"PST8PDT", 313028..314322),
    (r"ROC", 327774..328285),
    (r"ROK", 328285..328700),
    (r"Singapore", 328700..328956),
    (r"Turkey", 328956..330156),
    (r"UCT", 330156..330267),
    (r"Universal", 341211..341322),
    (r"US/Alaska", 330267..331244),
    (r"US/Aleutian", 331244..332213),
    (r"US/Arizona", 332213..332453),
    (r"US/Central", 332453..334207),
    (r"US/East-Indiana", 334207..334738),
    (r"US/Eastern", 334738..336482),
    (r"US/Hawaii", 336482..336703),
    (r"US/Indiana-Starke", 336703..337719),
    (r"US/Michigan", 337719..338618),
    (r"US/Mountain", 338618..339660),
    (r"US/Pacific", 339660..340954),
    (r"US/Samoa", 340954..341100),
    (r"UTC", 341100..341211),
    (r"W-SU", 341322..342230),
    (r"WET", 342230..343693),
    (r"Zulu", 343693..343804),
];
