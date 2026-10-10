#include <cmath>
#include <cstdio>
#include <initializer_list>
constexpr float WAVE_MOD_MAGNITUDE=20, WAVE_MOD_HEIGHT=38;
struct PlayerOptions { enum { ACCEL_WAVE, ACCEL_WAVE_PERIOD }; };
float Wave(float fYOffset, float amount, float period) {
    float fAccels[2]={amount,period};
    float fYAdjust=0;
  if (fAccels[PlayerOptions::ACCEL_WAVE] != 0) {
    fYAdjust +=
        fAccels[PlayerOptions::ACCEL_WAVE] * WAVE_MOD_MAGNITUDE *
        std::sin(
            fYOffset /
            ((fAccels[PlayerOptions::ACCEL_WAVE_PERIOD] * WAVE_MOD_HEIGHT) +
             WAVE_MOD_HEIGHT));
  }
    return fYOffset+fYAdjust;
}
int main() {
    for(float amount: {0.f, 1.f, -0.75f, 0.25f})
    for(float period: {0.f, 0.5f, -0.5f, 2.f})
    for(float travel: {0.f, 32.f, 128.f, 384.f, 768.f})
        std::printf("{\"amount\":%.9g,\"period\":%.9g,\"travel\":%.9g,\"y\":%.9g}\n",amount,period,travel,Wave(travel,amount,period));
}
