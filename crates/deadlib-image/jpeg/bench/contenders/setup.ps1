param([string]$Work = 'target/jpeg-codec-comparison')

$ErrorActionPreference = 'Stop'
$bench = Split-Path $PSScriptRoot
$repo = (Resolve-Path (Join-Path $bench '../../../..')).Path

function Invoke-BuildStep {
    $program, $arguments = $args
    & $program @arguments
    if ($LASTEXITCODE -ne 0) { throw "$program exited with $LASTEXITCODE" }
}

Push-Location $repo
try {
    $base = (New-Item -Force -ItemType Directory $Work).FullName
    $upstream = (New-Item -Force -ItemType Directory "$base/upstream").FullName
    foreach ($source in (Get-Content "$PSScriptRoot/upstream.tsv" | ConvertFrom-Csv -Delimiter "`t")) {
        $checkout = "$upstream/$($source.name)"
        if (!(Test-Path "$checkout/.git")) {
            Invoke-BuildStep git init -q $checkout
            Invoke-BuildStep git -C $checkout remote add origin $source.url
            Invoke-BuildStep git -C $checkout fetch --depth 1 origin $source.commit
            Invoke-BuildStep git -C $checkout checkout --detach FETCH_HEAD
        }
        if ($source.name -eq 'jpegli') {
            Invoke-BuildStep git -C $checkout submodule update --init --recursive --depth 1
        }
        if ((git -C $checkout rev-parse HEAD) -ne $source.commit -or (git -C $checkout status --porcelain)) {
            throw "Unexpected upstream source state in $checkout"
        }
    }

    $generator = @('-G', 'Visual Studio 17 2022', '-A', 'x64')
    Invoke-BuildStep cmake -S "$upstream/turbo" -B "$base/turbo-build" @generator `
        -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_SIMD=ON -DREQUIRE_SIMD=ON
    Invoke-BuildStep cmake --build "$base/turbo-build" --config Release --target jpeg-static cjpeg-static --parallel 8
    Invoke-BuildStep cmake -S "$upstream/mozjpeg" -B "$base/moz-static" @generator `
        -DBUILD_SHARED_LIBS=OFF -DPNG_SUPPORTED=OFF -DWITH_SIMD=ON -DREQUIRE_SIMD=ON -DWITH_ARITH_DEC=ON -DWITH_ARITH_ENC=ON
    Invoke-BuildStep cmake --build "$base/moz-static" --config Release --target jpeg-static --parallel 8
    Invoke-BuildStep cmake -S "$upstream/jpegli" -B "$base/jpegli-build" @generator `
        -DBUILD_TESTING=OFF -DJPEGLI_ENABLE_TOOLS=OFF -DJPEGLI_ENABLE_SJPEG=OFF -DJPEGLI_ENABLE_OPENEXR=OFF `
        -DJPEGLI_ENABLE_SKCMS=OFF -DJPEGLI_ENABLE_TCMALLOC=OFF -DJPEGLI_ENABLE_JNI=OFF -DJPEGLI_ENABLE_BENCHMARK=OFF `
        -DJPEGLI_ENABLE_DOXYGEN=OFF -DJPEGLI_ENABLE_MANPAGES=OFF
    Invoke-BuildStep cmake --build "$base/jpegli-build" --config Release --target jpegli-static --parallel 8
    Invoke-BuildStep cmake -S $PSScriptRoot -B "$base/bridge-build" @generator "-DWORK=$base" "-DWUFFS=$upstream/wuffs"
    Invoke-BuildStep cmake --build "$base/bridge-build" --config Release --parallel 8
    Invoke-BuildStep cargo build --manifest-path "$bench/Cargo.toml" --release --locked --features comparison `
        --bin compare --target-dir "$base/rust-build"
    Invoke-BuildStep python "$PSScriptRoot/corpus.py" $repo $base
} finally {
    Pop-Location
}
