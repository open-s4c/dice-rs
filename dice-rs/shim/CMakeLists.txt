cmake_minimum_required(VERSION 3.20)
project(
  shim
  LANGUAGES C
  VERSION 1.0)

add_subdirectory(../../dice ${CMAKE_CURRENT_BINARY_DIR}/dice)
include_directories(../../dice/include .)
find_package(libvsync 4.2 CONFIG REQUIRED)
link_libraries(libvsync::vsync)
add_library(shim STATIC log_shim.c)
target_compile_options(shim PRIVATE -fPIC)
