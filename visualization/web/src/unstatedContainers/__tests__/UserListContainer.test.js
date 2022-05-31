import UserListContainer from "../UserListContainer";

describe("UserListContainer", async () => {
    it("UserListContainer created", async () => {
        let userList = new UserListContainer();
        expect(userList.state.loading).toEqual(false);
        expect(userList.state.users).toEqual([]);
        expect(userList.state.roles).toEqual([]);

        await userList.loadUserList();
    });
});
