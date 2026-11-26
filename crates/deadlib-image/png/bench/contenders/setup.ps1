param([string]$Work = 'target/png-codec-comparison')

$ErrorActionPreference = 'Stop'
$bench = Split-Path $PSScriptRoot
$repo = (Resolve-Path (Join-Path $bench '../../../..')).Path

function Invoke-BuildStep {
    $program, $arguments = $args
    & $program @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$program failed with exit code $LASTEXITCODE"
    }
}

Push-Location $repo
try {
    $base = (New-Item -Force -ItemType Directory $Work).FullName
    $upstream = (New-Item -Force -ItemType Directory "$base/upstream").FullName
    $pins = Get-Content "$PSScriptRoot/upstream.tsv" | ConvertFrom-Csv -Delimiter "`t"
    foreach ($source in $pins) {
        $checkout = "$upstream/$($source.name)"
        if (!(Test-Path "$checkout/.git")) {
            Invoke-BuildStep git init -q $checkout
            Invoke-BuildStep git -C $checkout remote add origin $source.url
            Invoke-BuildStep git -C $checkout fetch --depth 1 origin $source.commit
            Invoke-BuildStep git -C $checkout checkout --detach FETCH_HEAD
        }
        if ((git -C $checkout rev-parse HEAD) -ne $source.commit -or (git -C $checkout status --porcelain)) {
            throw "Unexpected upstream source state in $checkout"
        }
    }

    Invoke-BuildStep cmake -S "$upstream/zlibng" -B "$base/zlib-build" -G 'Visual Studio 17 2022' -A x64 `
        -DZLIB_COMPAT=ON -DBUILD_TESTING=OFF -DBUILD_SHARED_LIBS=OFF "-DCMAKE_INSTALL_PREFIX=$base/zlib"
    Invoke-BuildStep cmake --build "$base/zlib-build" --config Release --target INSTALL --parallel 8
    Invoke-BuildStep cmake -S $PSScriptRoot -B "$base/c-build" -G 'Visual Studio 17 2022' -A x64 `
        "-DUPSTREAM=$upstream" "-DZLIB_ROOT=$base/zlib" "-DZLIB_LIBRARY=$base/zlib/lib/zlibstatic.lib"
    Invoke-BuildStep cmake --build "$base/c-build" --config Release --parallel 8

    $env:PNG_COMPARISON_LIB = "$base/c-build/Release"
    Invoke-BuildStep cargo build --release --locked --manifest-path "$bench/Cargo.toml" --features comparison `
        --target-dir "$base/rust-build" --bin compare
    Copy-Item "$env:PNG_COMPARISON_LIB/png_contenders.dll" "$base/rust-build/release"

    if (!(Test-Path "$base/formats/manifest.json")) {
        Invoke-BuildStep python "$bench/../scripts/generate_decode_corpus.py" --output "$base/formats"
    }
    Invoke-BuildStep python "$PSScriptRoot/corpus.py" $repo $base
} finally {
    Pop-Location
}
