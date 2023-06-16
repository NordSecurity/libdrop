import typing, os

from . import action, ffi
from .logger import logger

from . import action


class ActionList:
    def __init__(self, actions: typing.List[action.Action]):
        self._actions = actions

    async def run(self, drop: ffi.Drop):
        for act in self._actions:
            logger.info(f"Running action: {act}")
            await act.run(drop)


class Scenario:
    def __init__(
        self,
        id: str,
        desc: str,
        action_list: typing.Dict[str, ActionList],
        dbpath: str = ":memory:",
    ):
        self._id = id
        self._desc = desc
        self._action_list = action_list
        self._dbpath = dbpath

    def id(self):
        return self._id

    async def run(self, runner: str, drop: ffi.Drop, addr: str):
        logger.info(f'Scenario: "{self._desc}"')

        try:
            drop.start(addr, runner, self._dbpath)

            await self._action_list[runner].run(drop)
            os.seteuid(0)  # restore privileges, they might have been changed
        except Exception as e:
            logger.debug(f"Action threw an exception: {e}")
            raise

    def str(self):
        return f"Scenario({self._id}, {self._desc}, {self._action_list})"

    def runners(self):
        return self._action_list.keys()
