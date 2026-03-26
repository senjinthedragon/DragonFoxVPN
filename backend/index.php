<?php
/**
 * index.php - DragonFoxVPN: Backend web UI
 * Copyright (c) 2026 Senjin the Dragon.
 * https://github.com/senjinthedragon/DragonFoxVPN
 * Licensed under the MIT License.
 * See LICENSE for full license information.
 *
 * PHP web UI served by the Raspberry Pi gateway. Displays available VPN
 * locations grouped by continent with flag icons, and triggers location
 * switches by calling switch-openvpn.sh via sudo.
 */

// Load configuration from the shared config file
$_cfg        = @parse_ini_file('/etc/dragonfoxvpn/config.conf') ?: [];
$CONF_PREFIX = $_cfg['CONF_PREFIX'] ?? '';

$locationsFile = "/etc/openvpn/locations.txt";
$wrapperScript = "/usr/local/bin/switch-openvpn.sh";
$activeLink    = "/etc/openvpn/client/active.conf";
$msg = "";

// Refresh the location list only when the .ovpn directory has changed
// (new files added or removed). Comparing directory mtime against locations.txt
// mtime avoids running a sudo subprocess on every page load.
$ovpnDir  = "/etc/openvpn/client";
$locMtime = file_exists($locationsFile) ? filemtime($locationsFile) : 0;
$dirMtime = is_dir($ovpnDir)            ? filemtime($ovpnDir)       : 0;
if ($dirMtime > $locMtime) {
    exec("sudo " . escapeshellcmd($wrapperScript) . " --refresh 2>&1");
}

if ($_SERVER['REQUEST_METHOD'] === 'POST' && isset($_POST['location'])) {
    $target = basename($_POST['location']);
    exec("sudo " . escapeshellcmd($wrapperScript) . " " . escapeshellarg($target) . " 2>&1", $output, $ret);
    $msg = $ret === 0 ? "" : "Error switching:\n" . implode("\n", $output);
}

// Load available locations
$locations = file_exists($locationsFile)
    ? file($locationsFile, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES)
    : [];

// Determine current active config
$current = is_link($activeLink) ? basename(readlink($activeLink), ".ovpn") : null;

// Strip filename prefix and protocol suffix (_udp/_tcp) for display
function prettyName($loc) {
    global $CONF_PREFIX;
    $name = $CONF_PREFIX ? preg_replace('/^' . preg_quote($CONF_PREFIX, '/') . '/', '', $loc) : $loc;
    $name = preg_replace('/_(udp|tcp)$/', '', $name);
    $name = str_replace('_', ' ', $name);
    return ucwords($name);
}

function normalizeCountry($name) {
    return strtolower(trim($name));
}

// Continent mapping
$continentMap = [
    "albania"=>"Europe","algeria"=>"Africa","andorra"=>"Europe","argentina"=>"South America",
    "armenia"=>"Asia","australia"=>"Oceania","austria"=>"Europe","azerbaijan"=>"Asia",
    "bahamas"=>"North America","bangladesh"=>"Asia","belarus"=>"Europe","belgium"=>"Europe",
    "bermuda"=>"North America","bhutan"=>"Asia","bolivia"=>"South America",
    "bosnia and herzegovina"=>"Europe","brazil"=>"South America","brunei"=>"Asia",
    "bulgaria"=>"Europe","cambodia"=>"Asia","canada"=>"North America","cayman islands"=>"North America",
    "chile"=>"South America","colombia"=>"South America","costa rica"=>"North America","croatia"=>"Europe",
    "cuba"=>"North America","cyprus"=>"Europe","czech republic"=>"Europe","denmark"=>"Europe",
    "dominican republic"=>"North America","ecuador"=>"South America","egypt"=>"Africa","estonia"=>"Europe",
    "finland"=>"Europe","france"=>"Europe","georgia"=>"Asia","germany"=>"Europe","ghana"=>"Africa",
    "greece"=>"Europe","guam"=>"Oceania","guatemala"=>"North America","honduras"=>"North America",
    "hong kong"=>"Asia","hungary"=>"Europe","iceland"=>"Europe","india"=>"Asia","indonesia"=>"Asia",
    "ireland"=>"Europe","isle of man"=>"Europe","israel"=>"Asia","italy"=>"Europe","jamaica"=>"North America",
    "japan"=>"Asia","jersey"=>"Europe","kazakhstan"=>"Asia","kenya"=>"Africa","laos"=>"Asia","latvia"=>"Europe",
    "lebanon"=>"Asia","liechtenstein"=>"Europe","lithuania"=>"Europe","luxembourg"=>"Europe","macau"=>"Asia",
    "malaysia"=>"Asia","malta"=>"Europe","mexico"=>"North America","moldova"=>"Europe","monaco"=>"Europe",
    "mongolia"=>"Asia","montenegro"=>"Europe","morocco"=>"Africa","myanmar"=>"Asia","nepal"=>"Asia",
    "netherlands"=>"Europe","new zealand"=>"Oceania","north macedonia"=>"Europe","norway"=>"Europe",
    "pakistan"=>"Asia","panama"=>"North America","peru"=>"South America","philippines"=>"Asia","poland"=>"Europe",
    "portugal"=>"Europe","puerto rico"=>"North America","romania"=>"Europe","serbia"=>"Europe","singapore"=>"Asia",
    "slovakia"=>"Europe","slovenia"=>"Europe","south africa"=>"Africa","south korea"=>"Asia","spain"=>"Europe",
    "sri lanka"=>"Asia","sweden"=>"Europe","switzerland"=>"Europe","taiwan"=>"Asia","thailand"=>"Asia",
    "trinidad and tobago"=>"North America","turkey"=>"Asia","uk"=>"Europe","ukraine"=>"Europe",
    "uruguay"=>"South America","usa"=>"North America","uzbekistan"=>"Asia","venezuela"=>"South America",
    "vietnam"=>"Asia"
];

// ISO codes for flag-icons CSS library
$countryCodes = [
    "albania"=>"al","algeria"=>"dz","andorra"=>"ad","argentina"=>"ar","armenia"=>"am",
    "australia"=>"au","austria"=>"at","azerbaijan"=>"az","bahamas"=>"bs","bangladesh"=>"bd",
    "belarus"=>"by","belgium"=>"be","bermuda"=>"bm","bhutan"=>"bt","bolivia"=>"bo",
    "bosnia and herzegovina"=>"ba","brazil"=>"br","brunei"=>"bn","bulgaria"=>"bg","cambodia"=>"kh",
    "canada"=>"ca","cayman islands"=>"ky","chile"=>"cl","colombia"=>"co","costa rica"=>"cr",
    "croatia"=>"hr","cuba"=>"cu","cyprus"=>"cy","czech republic"=>"cz","denmark"=>"dk",
    "dominican republic"=>"do","ecuador"=>"ec","egypt"=>"eg","estonia"=>"ee","finland"=>"fi",
    "france"=>"fr","georgia"=>"ge","germany"=>"de","ghana"=>"gh","greece"=>"gr","guam"=>"gu",
    "guatemala"=>"gt","honduras"=>"hn","hong kong"=>"hk","hungary"=>"hu","iceland"=>"is","india"=>"in",
    "indonesia"=>"id","ireland"=>"ie","isle of man"=>"im","israel"=>"il","italy"=>"it",
    "jamaica"=>"jm","japan"=>"jp","jersey"=>"je","kazakhstan"=>"kz","kenya"=>"ke","laos"=>"la",
    "latvia"=>"lv","lebanon"=>"lb","liechtenstein"=>"li","lithuania"=>"lt","luxembourg"=>"lu",
    "macau"=>"mo","malaysia"=>"my","malta"=>"mt","mexico"=>"mx","moldova"=>"md","monaco"=>"mc",
    "mongolia"=>"mn","montenegro"=>"me","morocco"=>"ma","myanmar"=>"mm","nepal"=>"np",
    "netherlands"=>"nl","new zealand"=>"nz","north macedonia"=>"mk","norway"=>"no","pakistan"=>"pk",
    "panama"=>"pa","peru"=>"pe","philippines"=>"ph","poland"=>"pl","portugal"=>"pt","puerto rico"=>"pr",
    "romania"=>"ro","serbia"=>"rs","singapore"=>"sg","slovakia"=>"sk","slovenia"=>"si",
    "south africa"=>"za","south korea"=>"kr","spain"=>"es","sri lanka"=>"lk","sweden"=>"se",
    "switzerland"=>"ch","taiwan"=>"tw","thailand"=>"th","trinidad and tobago"=>"tt",
    "turkey"=>"tr","uk"=>"gb","ukraine"=>"ua","uruguay"=>"uy","usa"=>"us","uzbekistan"=>"uz",
    "venezuela"=>"ve","vietnam"=>"vn"
];

$continentFlags = [
    "Europe"=>"🌍","Africa"=>"🌍","Asia"=>"🌏","Oceania"=>"🌏",
    "North America"=>"🌎","South America"=>"🌎","Other"=>"🌐"
];

// Group locations by continent
$grouped = [];
foreach ($locations as $loc) {
    $name = prettyName($loc);
    $parts = explode(" - ", $name, 2);
    $countryKey = normalizeCountry($parts[0]);
    $continent = $continentMap[$countryKey] ?? "Other";
    $flagSpan = isset($countryCodes[$countryKey])
        ? '<span class="fi fi-' . $countryCodes[$countryKey] . ' fis"></span>'
        : '';
    $grouped[$continent][] = [
        "value" => $loc,
        "label" => $name,
        "flag"  => $flagSpan
    ];
}
?>
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>DragonFoxVPN Switcher</title>
    <link rel="stylesheet" href="vpn.css">
    <link rel="stylesheet" href="flag-icons/css/flag-icons.min.css">
</head>
<body>
<h1>DragonFoxVPN</h1>
<h3>Switch Location</h3>
<?php if (!empty($msg)) echo "<p class='error'><strong>" . htmlspecialchars($msg) . "</strong></p>"; ?>

<form method="post" id="vpnForm">
    <div class="dropdown">
        <div class="dropdown-btn" id="selectedBtn">Select a VPN location</div>
        <div class="dropdown-content">
            <?php foreach ($grouped as $continent => $items): ?>
                <div class="optgroup-label">
                    <?php echo $continentFlags[$continent] . " " . htmlspecialchars($continent); ?>
                </div>
                <?php foreach ($items as $item): ?>
                    <div class="dropdown-item<?php echo ($item['value'] === $current) ? ' active' : ''; ?>"
                         data-value="<?php echo htmlspecialchars($item['value']); ?>">
                        <?php echo $item['flag'] . " " . htmlspecialchars($item['label']); ?>
                    </div>
                <?php endforeach; ?>
            <?php endforeach; ?>
        </div>
    </div>
    <input type="hidden" name="location" id="vpnLocation" value="">
</form>

<script>
const dropdownBtn = document.getElementById('selectedBtn');
const dropdownContent = document.querySelector('.dropdown-content');
const hiddenInput = document.getElementById('vpnLocation');
const items = document.querySelectorAll('.dropdown-item');
const form = document.getElementById('vpnForm');

dropdownBtn.addEventListener('click', () => {
    dropdownContent.style.display = dropdownContent.style.display === 'block' ? 'none' : 'block';
});

items.forEach(item => {
    item.addEventListener('click', () => {
        hiddenInput.value = item.dataset.value;
        items.forEach(i => i.classList.remove('active'));
        item.classList.add('active');
        dropdownBtn.innerHTML = item.innerHTML;
        dropdownContent.style.display = 'none';
        form.submit();
    });
});

const currentItem = document.querySelector('.dropdown-item.active');
if (currentItem) {
    hiddenInput.value = currentItem.dataset.value;
    dropdownBtn.innerHTML = currentItem.innerHTML;
}
</script>
</body>
</html>
