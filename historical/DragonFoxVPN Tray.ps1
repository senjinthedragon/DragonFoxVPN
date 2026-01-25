# Requires -Version 5.1

$global:hasConsole = $Host.Name -eq "ConsoleHost"
if ($global:hasConsole) {
    [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# --- Configuration ---
$global:adapterName = "Ethernet"
$global:manualDisable = $false
$ispGateway  = "10.0.0.1"
$vpnGateway  = "10.0.0.20"
$dnsServer   = "10.0.0.20"
$checkInterval = 3

# --- Icon Bitmaps (Embedded) ---
# We'll create simple colored icons dynamically for Tray
function New-ColoredIcon($color) {
    $bitmap = New-Object System.Drawing.Bitmap 16,16
    $g = [System.Drawing.Graphics]::FromImage($bitmap)
    $brush = New-Object System.Drawing.SolidBrush $color
    $g.FillEllipse($brush, 0,0,16,16)
    $g.Dispose()
    $icon = [System.Drawing.Icon]::FromHandle($bitmap.GetHicon())
    return $icon
}

$iconConnected = New-ColoredIcon([System.Drawing.Color]::Green)
$iconDisabled = New-ColoredIcon([System.Drawing.Color]::Yellow)
$iconDropped = New-ColoredIcon([System.Drawing.Color]::Red)
$iconInfo = New-ColoredIcon([System.Drawing.Color]::Blue)

# --- Console Logging Helper ---
function Log-Success($msg) { if ($global:hasConsole) {Write-Host $msg -ForegroundColor Green } }
function Log-Info($msg) { if ($global:hasConsole) {Write-Host $msg -ForegroundColor Blue } }
function Log-Warning($msg) { if ($global:hasConsole) {Write-Host $msg -ForegroundColor Yellow } }
function Log-Error($msg) { if ($global:hasConsole) {Write-Host $msg -ForegroundColor Red } }

function Flash-DropIcon {
    $flashCount = 3
    $flashInterval = 300  # milliseconds
    $i = 0

    $flashTimer = New-Object System.Windows.Forms.Timer
    $flashTimer.Interval = $flashInterval
    $flashTimer.add_Tick({
        if ($i -ge $flashCount * 2) {
            $flashTimer.Stop()
            $notifyIcon.Icon = $iconDropped
            $flashTimer.Dispose()
        } else {
            $notifyIcon.Icon = if ($i % 2 -eq 0) { $iconInfo } else { $iconDropped }
            $i++
        }
    })
    $flashTimer.Start()
}

function Get-ActiveNetworkAdapter {
    $defaultRoute = Get-NetRoute -DestinationPrefix "0.0.0.0/0" -ErrorAction SilentlyContinue |
                    Sort-Object RouteMetric | Select-Object -First 1
    if ($defaultRoute) {
        $adapter = Get-NetAdapter -InterfaceIndex $defaultRoute.InterfaceIndex
        $global:adapterName = $adapter.Name
        Log-Info "Detected active adapter: $global:adapterName"
        return $true
    } else {
        Log-Warning "Could not detect an active network adapter. Please specify one manually."
        return $false
    }
}

function Set-TrayMenuState($state) {
    if ($state -eq "Enabled") {
        $menuItemEnable.Enabled = $false
        $menuItemDisable.Enabled = $true
    } else {
        $menuItemEnable.Enabled = $true
        $menuItemDisable.Enabled = $false
    }
}

function Flush-Dns { Start-Process "ipconfig" -ArgumentList "/flushdns" -WindowStyle Hidden -Wait }

function Enable-VPNRouting {
    if ($global:manualDisable) { return }

	try {
        # Visual feedback
        $notifyIcon.Icon = $iconInfo
        $notifyIcon.Text = "DragonFoxVPN: Connecting..."
		Log-Info "Enabling DragonFoxVPN routing..."

        # Disable IPv6
        Set-NetAdapterBinding -Name $global:adapterName -ComponentID ms_tcpip6 -Enabled $false | Out-Null

        # Set DNS to Pi
        Set-DnsClientServerAddress -InterfaceAlias $global:adapterName -ServerAddresses $dnsServer | Out-Null

        # Remove any existing default routes via other gateways
        try {
            Get-NetRoute -InterfaceAlias $global:adapterName -DestinationPrefix "0.0.0.0/0" |
                Remove-NetRoute -Confirm:$false | Out-Null
        }
        catch {}

        # Add default route via Pi
        New-NetRoute -InterfaceAlias $global:adapterName -DestinationPrefix "0.0.0.0/0" -NextHop $vpnGateway | Out-Null

        # Flush DNS cache
        Flush-Dns

        Set-TrayMenuState("Enabled")
		$notifyIcon.Icon = $iconConnected
        $notifyIcon.Text = "DragonFoxVPN: Connected"
        $notifyIcon.ShowBalloonTip(3000, "VPN Tray", "DragonFoxVPN enabled. Remember to restart your browser to use it!", [System.Windows.Forms.ToolTipIcon]::Info)
        Log-Success "DragonFoxVPN enabled successfully."
	}
    catch {
        Log-Error "Failed to enable DragonFoxVPN routing: $_"
        Disable-VPNRouting-Manual
    }
}

function Disable-VPNRouting-Manual {
    $global:manualDisable = $true
    try {
        $notifyIcon.Icon = $iconInfo
        $notifyIcon.Text = "DragonFoxVPN: Restoring..."
        Log-Info "Disabling DragonFoxVPN routing..."

        # Re-enable DHCP for IPv4 (and IPv6 should restore automatically)
        Set-NetIPInterface -InterfaceAlias $global:adapterName -Dhcp Enabled | Out-Null

        # Re-enable IPv6
        Set-NetAdapterBinding -Name $global:adapterName -ComponentID ms_tcpip6 -Enabled $true | Out-Null

        # Reset DNS to DHCP
        Set-DnsClientServerAddress -InterfaceAlias $global:adapterName -ResetServerAddresses | Out-Null

        # Remove any lingering VPN routes
        try {
            Get-NetRoute -InterfaceAlias $global:adapterName -NextHop $vpnGateway | Remove-NetRoute -Confirm:$false
        }
        catch {}

        # Flush DNS cache
        Flush-Dns

        Set-TrayMenuState("Disabled")
        $notifyIcon.Icon = $iconDisabled
        $notifyIcon.Text = "DragonFoxVPN: Disconnected"
        $notifyIcon.ShowBalloonTip(3000, "VPN Tray", "DragonFoxVPN disabled. Remember to restart your browser!", [System.Windows.Forms.ToolTipIcon]::Info)
        Log-Warning "DragonFoxVPN disabled."
    }
	catch {
        Log-Error "Failed to restore network settings: $_"
    }
}

function Drop-Route {
	try {
        Get-NetRoute -InterfaceAlias $global:adapterName -DestinationPrefix "0.0.0.0/0" -NextHop $vpnGateway -ErrorAction SilentlyContinue |
            Remove-NetRoute -Confirm:$false -ErrorAction SilentlyContinue | Out-Null
        $notifyIcon.Icon = $iconDropped
        $notifyIcon.Text = "DragonFoxVPN: Dropped"
        $notifyIcon.ShowBalloonTip(3000, "VPN Tray", "DragonFoxVPN connection dropped. Internet access disabled to prevent leaks.", [System.Windows.Forms.ToolTipIcon]::Warning)
        Log-Warning "DragonFoxVPN connection dropped. Internet disabled."
		Flash-DropIcon
    }
    catch {
        Log-Error "Could not drop network connection. Your traffic is in flux and might be public!"
    }
}

# --- Function to check VPN status via first hop ---
function Test-VPNActive {
    try {
       $traceresult = & cmd /c "tracert -d -h 1 8.8.8.8" 2>$null
        if ($traceresult) {
            $firstLine = ($traceresult | Select-Object -Skip 1 | Select-Object -First 1)
            if ($firstLine -match '(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})') {
                $firstHopIp = $matches[1]
                if ($firstHopIp -eq $ispGateway) { return $false }
                elseif ($firstHopIp -eq $vpnGateway) { return $true }
                else { return $true }  # unknown internal hop, assume VPN active
            } else {
                return $false
            }
        } else { return $false }
    } catch {
        return $false
    }
}

$currentProcess = Get-Process -Id $PID
$runningInstances = Get-Process | Where-Object {
    $_.Id -ne $currentProcess.Id -and $_.Path -eq $currentProcess.Path
}
if ($runningInstances) {
    Log-Warning "Another instance of DragonFoxVPN Tray is already running. Exiting."
    exit
}

[System.AppDomain]::CurrentDomain.add_ProcessExit({
    Log-Info "Cleaning up network settings before exiting..."
    Disable-VPNRouting-Manual
    $notifyIcon.Dispose()
    $iconConnected.Dispose()
    $iconDisabled.Dispose()
    $iconDropped.Dispose()
    $iconInfo.Dispose()
})

if (-not (Get-ActiveNetworkAdapter)) {
    Log-Error "No active network adapter found. Exiting."
    exit
}

# --- Tray Icon ---
$notifyIcon = New-Object System.Windows.Forms.NotifyIcon
$notifyIcon.Icon = [System.Drawing.SystemIcons]::Information
$notifyIcon.Visible = $true
$notifyIcon.Text = "DragonFoxVPN Tray"

$contextMenu = New-Object System.Windows.Forms.ContextMenu
$menuItemEnable = New-Object System.Windows.Forms.MenuItem "Enable VPN"
$menuItemDisable = New-Object System.Windows.Forms.MenuItem "Disable VPN"
$menuItemExit = New-Object System.Windows.Forms.MenuItem "Exit"
$contextMenu.MenuItems.Add($menuItemEnable) | Out-Null
$contextMenu.MenuItems.Add($menuItemDisable) | Out-Null
$contextMenu.MenuItems.Add($menuItemExit) | Out-Null
$notifyIcon.ContextMenu = $contextMenu
$menuItemEnable.add_Click({
    $global:manualDisable = $false
    Enable-VPNRouting
})
$menuItemDisable.add_Click({ Disable-VPNRouting-Manual })
$menuItemExit.add_Click({
    $notifyIcon.Visible = $false
    [System.Windows.Forms.Application]::Exit()
})

# --- Initial VPN Check ---
if (Test-Connection -ComputerName $vpnGateway -Count 1 -Quiet) {
    Enable-VPNRouting
	$global:currentVPNState = "Connected"
} else {
    Drop-Route
	$global:currentVPNState = "Dropped"
}

# GUI-safe timer for keep-alive
$global:timer = New-Object System.Windows.Forms.Timer
$global:timer.Interval = $checkInterval * 1000  # milliseconds
$global:timer.add_Tick({
    try {
        $vpnActive = Test-VPNActive
        $routeExists = Get-NetRoute -InterfaceAlias $global:adapterName -NextHop $vpnGateway -ErrorAction SilentlyContinue

        if ($vpnActive -and -not $routeExists) {
            try { 
                Enable-VPNRouting
                if ($global:currentVPNState -ne "Connected") { $global:currentVPNState = "Connected" }
            } 
            catch { Log-Error "Error enabling VPN routing: $_" }
        }
        elseif (-not $vpnActive -and $routeExists) {
            try { 
                Drop-Route
                if ($global:currentVPNState -ne "Dropped") { $global:currentVPNState = "Dropped" }
            } 
            catch { Log-Error "Error dropping VPN route: $_" }
        }
        # Optional: log heartbeat
        # Log-Info "Timer tick completed. VPN state: $global:currentVPNState"
    } 
    catch { 
        Log-Error "Timer tick failed: $_"
    }
})
$global:timer.Start()

# Start the WinForms message loop (blocks the main thread)
[System.Windows.Forms.Application]::Run()
