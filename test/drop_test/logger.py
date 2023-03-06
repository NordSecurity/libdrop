import logging

logger = logging.getLogger("global")

handler = logging.StreamHandler()
handler.setFormatter(
    logging.Formatter("%(asctime)s:%(name)s:%(levelname)s:%(message)s")
)
logger.setLevel(logging.DEBUG)
logger.addHandler(handler)
