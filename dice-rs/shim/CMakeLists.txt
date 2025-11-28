cmake_minimum_required(VERSION 3.20)
project(shim LANGUAGES C)

add_subdirectory(../../dice dice)
find_package(libvsync 4.2 CONFIG REQUIRED)
link_libraries(libvsync::vsync dice.h)
add_compile_options(-fPIC)

add_library(shim STATIC log_shim.c)
