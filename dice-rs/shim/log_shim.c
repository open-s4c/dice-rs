#include <dice/log.h>
#include <log_shim.h>
#include <unistd.h>

void dice_log_write(int level, const char *message) {
    if (level <= LOG_LEVEL_) {
        LOG_LOCK_ACQUIRE;
        log_printf(LOG_PREFIX);
        log_printf("%s", message);
        log_printf(LOG_SUFFIX);
        LOG_LOCK_RELEASE;
    }
}
