#include "../Abi.hpp"
#include <iostream>

namespace {
std::string hostHash;
}

extern "C" const char* __hyprland_api_get_hash() {
    return hostHash.c_str();
}

int main() {
    constexpr auto expected = "11cbc6c577b84ffb1edf4df3fc5b6128c3ae6947_aq_0.16_hu_0.14_hg_0.6_hc_0.1_hlg_0.6";
    if (J2barABI::expectedHash() != expected)
        return 1;
    hostHash = expected;
    if (!J2barABI::matchesHost())
        return 2;
    hostHash = "different-source_aq_0.16_hu_0.14_hg_0.6_hc_0.1_hlg_0.6";
    if (J2barABI::matchesHost())
        return 3;
    hostHash = "11cbc6c577b84ffb1edf4df3fc5b6128c3ae6947_aq_0.15_hu_0.14_hg_0.6_hc_0.1_hlg_0.6";
    if (J2barABI::matchesHost())
        return 4;
    std::cout << "PASS: exact header hash, matching host, wrong source and wrong dependency hash\n";
}
